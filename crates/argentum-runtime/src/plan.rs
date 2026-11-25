// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Two tools that let the agent tell the owner where it stands.
//!
//! `update_plan` replaces the stages of the current task with the agent's own
//! list, shown in the app's Plan module and in `argentum chat`. `ask_user`
//! puts a question to the owner and ends the run: the reply is the question,
//! and the owner's answer arrives as their next message. Neither tool reads or
//! changes anything in the workspace, and neither grants authority.

use argentum_domain::{AppEvent, PlanStep, RunId};
use argentum_providers::{ModelToolCall, ModelToolDefinition, ModelToolResult};
use serde::Deserialize;
use serde_json::json;

use super::{RuntimeError, RuntimeService};

pub(super) const PLAN_TOOL_ID: &str = "update_plan";
pub(super) const ASK_TOOL_ID: &str = "ask_user";

pub(super) const MAX_PLAN_STEPS: usize = 12;
pub(super) const MAX_STEP_TITLE_BYTES: usize = 160;
pub(super) const MAX_STEP_DETAIL_BYTES: usize = 400;
pub(super) const MAX_QUESTION_BYTES: usize = 1_000;
pub(super) const MAX_CHOICES: usize = 4;
pub(super) const MAX_CHOICE_BYTES: usize = 80;

pub(super) fn plan_tool_definition() -> ModelToolDefinition {
    ModelToolDefinition {
        name: PLAN_TOOL_ID.into(),
        description: "Show the owner the stages of this task. Send the whole list each time, in order, and mark each stage pending, in_progress, done or blocked. Use it for work with three or more stages; update it as stages finish.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_PLAN_STEPS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string", "minLength": 1, "maxLength": MAX_STEP_TITLE_BYTES },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "done", "blocked"] },
                            "detail": { "type": "string", "maxLength": MAX_STEP_DETAIL_BYTES }
                        },
                        "required": ["title", "status"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["steps"],
            "additionalProperties": false
        }),
    }
}

pub(super) fn ask_tool_definition() -> ModelToolDefinition {
    ModelToolDefinition {
        name: ASK_TOOL_ID.into(),
        description: "Ask the owner one question when you cannot go on without their answer, such as a choice between approaches or a missing fact. The run stops here and the owner's answer comes as their next message. Give up to four short choices when the answer is one of a few.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "minLength": 1, "maxLength": MAX_QUESTION_BYTES },
                "choices": {
                    "type": "array",
                    "maxItems": MAX_CHOICES,
                    "items": { "type": "string", "minLength": 1, "maxLength": MAX_CHOICE_BYTES }
                }
            },
            "required": ["question"],
            "additionalProperties": false
        }),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanInput {
    steps: Vec<StepInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepInput {
    title: String,
    status: StepStatus,
    #[serde(default)]
    detail: String,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StepStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionInput {
    question: String,
    #[serde(default)]
    choices: Vec<String>,
}

/// A question the agent put to the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Question {
    pub text: String,
    pub choices: Vec<String>,
}

impl Question {
    /// The reply the owner sees: the question, then its choices.
    pub(super) fn reply_text(&self) -> String {
        let mut text = self.text.clone();
        if !self.choices.is_empty() {
            text.push_str("\n\n");
            for choice in &self.choices {
                text.push_str("- ");
                text.push_str(choice);
                text.push('\n');
            }
            text.pop();
        }
        text
    }
}

fn result(call_id: String, disposition: &'static str, detail: &str) -> ModelToolResult {
    ModelToolResult {
        images: Default::default(),
        call_id,
        content: json!({ "version": 1, "disposition": disposition, "detail": detail }).to_string(),
    }
}

/// Trimmed, bounded, single-paragraph text, or why it is not acceptable.
fn clean(text: &str, limit: usize, allow_newlines: bool) -> Result<String, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err("text must not be empty");
    }
    if text.len() > limit {
        return Err("text is too long");
    }
    if text
        .chars()
        .any(|c| c.is_control() && !(allow_newlines && matches!(c, '\n' | '\t')))
    {
        return Err("text contains unsupported control characters");
    }
    Ok(text.to_owned())
}

pub(super) fn parse_plan(arguments: &str) -> Result<Vec<PlanStep>, &'static str> {
    let input: PlanInput = serde_json::from_str(arguments).map_err(|_| {
        "arguments must be {\"steps\": [{\"title\": string, \"status\": \"pending\" | \"in_progress\" | \"done\" | \"blocked\", \"detail\"?: string}]}"
    })?;
    if input.steps.is_empty() {
        return Err("a plan needs at least one stage");
    }
    if input.steps.len() > MAX_PLAN_STEPS {
        return Err("a plan has at most 12 stages");
    }
    input
        .steps
        .into_iter()
        .enumerate()
        .map(|(index, step)| {
            let detail = if step.detail.trim().is_empty() {
                String::new()
            } else {
                clean(&step.detail, MAX_STEP_DETAIL_BYTES, false)?
            };
            Ok(PlanStep {
                id: format!("stage-{}", index + 1),
                title: clean(&step.title, MAX_STEP_TITLE_BYTES, false)?,
                detail: match step.status {
                    StepStatus::InProgress if detail.is_empty() => "In progress".into(),
                    _ => detail,
                },
                complete: step.status == StepStatus::Done,
                blocked: step.status == StepStatus::Blocked,
            })
        })
        .collect()
}

pub(super) fn parse_question(arguments: &str) -> Result<Question, &'static str> {
    let input: QuestionInput = serde_json::from_str(arguments)
        .map_err(|_| "arguments must be {\"question\": string, \"choices\"?: [string]}")?;
    if input.choices.len() > MAX_CHOICES {
        return Err("give at most four choices");
    }
    Ok(Question {
        text: clean(&input.question, MAX_QUESTION_BYTES, true)?,
        choices: input
            .choices
            .iter()
            .map(|choice| clean(choice, MAX_CHOICE_BYTES, false))
            .collect::<Result<_, _>>()?,
    })
}

impl RuntimeService {
    /// Shows the agent's stages for the run. A refusal goes back to the model.
    pub(super) fn record_agent_plan(
        &self,
        run: RunId,
        call: &ModelToolCall,
    ) -> Result<ModelToolResult, RuntimeError> {
        let steps = match parse_plan(&call.arguments) {
            Ok(steps) => steps,
            Err(reason) => return Ok(result(call.id.clone(), "rejected", reason)),
        };
        self.emit_for_run(AppEvent::PlanUpdated { run_id: run, steps }, run)?;
        Ok(result(
            call.id.clone(),
            "recorded",
            "The owner sees these stages. Send the whole list again when a stage changes.",
        ))
    }

    /// Records the question. The caller ends the run after this round.
    pub(super) fn record_agent_question(
        &self,
        run: RunId,
        call: &ModelToolCall,
    ) -> Result<(ModelToolResult, Option<Question>), RuntimeError> {
        let question = match parse_question(&call.arguments) {
            Ok(question) => question,
            Err(reason) => return Ok((result(call.id.clone(), "rejected", reason), None)),
        };
        self.emit_for_run(
            AppEvent::QuestionAsked {
                run_id: run,
                question: question.text.clone(),
                choices: question.choices.clone(),
            },
            run,
        )?;
        Ok((
            result(
                call.id.clone(),
                "asked",
                "The question is shown to the owner and this run ends now. Their answer arrives as their next message.",
            ),
            Some(question),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_keeps_its_order_and_maps_each_status() {
        let steps = parse_plan(
            r#"{"steps":[{"title":"Read the code","status":"done"},{"title":"Fix it","status":"in_progress"},{"title":"Ship","status":"blocked","detail":"Needs CI"}]}"#,
        )
        .unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].id, "stage-1");
        assert!(steps[0].complete && !steps[0].blocked);
        assert!(!steps[1].complete && steps[1].detail == "In progress");
        assert!(steps[2].blocked && steps[2].detail == "Needs CI");
    }

    #[test]
    fn a_bad_plan_is_refused_with_a_reason() {
        assert!(parse_plan(r#"{"steps":[]}"#).is_err());
        assert!(parse_plan(r#"{"steps":[{"title":"  ","status":"done"}]}"#).is_err());
        assert!(parse_plan(r#"{"steps":[{"title":"x","status":"later"}]}"#).is_err());
        assert!(parse_plan(r#"{"steps":[{"title":"x","status":"done","owner":"me"}]}"#).is_err());
        let thirteen = (0..13)
            .map(|i| format!(r#"{{"title":"s{i}","status":"pending"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_plan(&format!(r#"{{"steps":[{thirteen}]}}"#)).is_err());
        assert!(parse_plan(r#"{"steps":[{"title":"a\u0007b","status":"done"}]}"#).is_err());
    }

    #[test]
    fn a_question_shows_its_choices_under_it() {
        let question =
            parse_question(r#"{"question":"Which database?","choices":["SQLite","Postgres"]}"#)
                .unwrap();
        assert_eq!(
            question.reply_text(),
            "Which database?\n\n- SQLite\n- Postgres"
        );
        assert_eq!(
            parse_question(r#"{"question":"Go on?"}"#)
                .unwrap()
                .reply_text(),
            "Go on?"
        );
        assert!(parse_question(r#"{"question":""}"#).is_err());
        assert!(parse_question(r#"{"question":"q","choices":["a","b","c","d","e"]}"#).is_err());
    }

    #[test]
    fn both_tools_have_small_closed_schemas() {
        for definition in [plan_tool_definition(), ask_tool_definition()] {
            assert_eq!(definition.parameters["additionalProperties"], false);
            assert!(definition.description.len() < 320);
        }
    }
}
