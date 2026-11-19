// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! How a reply's shown reasoning records its model steps, so the chat can
//! show each step's thinking followed by the answer text that step wrote.
//!
//! The shown reasoning is split into steps by [`REASONING_STEP_MARK`]. A step
//! after the first starts with [`REASONING_STEP_ANCHOR`], the byte offset in
//! the reply text where that step's answer begins, and a line break. A tool
//! call is a line of its own: the anchor, `@`, the reply text offset when it
//! ran, and its call id. Only the shown copy carries these marks; nothing here
//! is sent to a provider.

/// Separates one model step's reasoning from the next.
pub const REASONING_STEP_BREAK: &str = "\n\n\u{2063}\n\n";
pub const REASONING_STEP_MARK: char = '\u{2063}';
/// Starts a step's anchor: the reply text offset where the step's answer begins.
pub const REASONING_STEP_ANCHOR: char = '\u{2064}';

/// The text that opens a new step whose answer begins at `offset`.
pub fn step_start(offset: usize) -> String {
    format!("{REASONING_STEP_BREAK}{REASONING_STEP_ANCHOR}{offset}\n")
}

/// Records that a tool call ran when the reply text was `offset` bytes long,
/// so the chat can show it at that point of the reply.
pub fn tool_mark(offset: usize, call_id: &str) -> String {
    format!("\n{REASONING_STEP_ANCHOR}@{offset} {call_id}\n")
}

/// The shown reasoning as plain text, for views that do not split a reply
/// into steps: each step's thinking, separated by a blank line.
pub fn plain_reasoning(reasoning: &str) -> String {
    let plain = parse(reasoning)
        .into_iter()
        .map(|section| section.body)
        .filter(|body| !body.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    // A streamed piece that opens a new step keeps the gap before it.
    if reasoning.trim_start().starts_with(REASONING_STEP_MARK) {
        format!("\n\n{plain}")
    } else {
        plain
    }
}

/// Where a tool call ran inside a step: the byte offset in the step's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPlace {
    pub offset: usize,
    pub call_id: String,
}

/// One model step of a reply: what it thought, what it answered, and the
/// tools it called, each at the point of the answer where it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyStep {
    pub reasoning: String,
    pub answer: String,
    pub tools: Vec<ToolPlace>,
}

struct Section {
    anchor: Option<usize>,
    body: String,
    /// Reply text offsets and call ids.
    tools: Vec<(usize, String)>,
}

fn parse(reasoning: &str) -> Vec<Section> {
    reasoning
        .split(REASONING_STEP_MARK)
        .map(|section| {
            let section = section.trim_start_matches(['\n', '\r', ' ']);
            let (anchor, rest) = match section.strip_prefix(REASONING_STEP_ANCHOR) {
                Some(rest) if !rest.starts_with('@') => {
                    let (number, body) = rest.split_once('\n').unwrap_or((rest, ""));
                    (number.trim().parse().ok(), body)
                }
                _ => (None, section),
            };
            let mut tools = Vec::new();
            let mut kept = Vec::new();
            for line in rest.split('\n') {
                let mark = line
                    .trim()
                    .strip_prefix(REASONING_STEP_ANCHOR)
                    .and_then(|mark| mark.strip_prefix('@'));
                match mark {
                    Some(mark) => {
                        let (offset, id) = mark.split_once(' ').unwrap_or((mark, ""));
                        if let Ok(offset) = offset.parse() {
                            tools.push((offset, id.trim().to_owned()));
                        }
                    }
                    None => kept.push(line),
                }
            }
            Section {
                anchor,
                body: kept.join("\n").trim().to_owned(),
                tools,
            }
        })
        .collect()
}

fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The reply as its model steps, in order. Reasoning stored before steps had
/// anchors gives one step with all of its reasoning, then the whole answer.
pub fn reply_steps(reasoning: &str, text: &str) -> Vec<ReplyStep> {
    let sections = parse(reasoning);
    if sections
        .iter()
        .all(|section| section.anchor.is_none() && section.tools.is_empty())
    {
        let reasoning = sections
            .into_iter()
            .map(|section| section.body)
            .filter(|body| !body.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        return vec![ReplyStep {
            reasoning,
            answer: text.trim().to_owned(),
            tools: Vec::new(),
        }];
    }
    let mut starts = Vec::with_capacity(sections.len());
    let mut last = 0;
    for section in &sections {
        last = section
            .anchor
            .map_or(last, |offset| floor_boundary(text, offset).max(last));
        starts.push(last);
    }
    sections
        .into_iter()
        .enumerate()
        .map(|(index, section)| {
            let start = starts[index];
            let end = starts.get(index + 1).copied().unwrap_or(text.len());
            let raw = &text[start..end];
            let leading = raw.len() - raw.trim_start().len();
            let answer = raw.trim();
            let tools = section
                .tools
                .into_iter()
                .map(|(offset, call_id)| ToolPlace {
                    offset: floor_boundary(
                        answer,
                        offset.clamp(start, end).saturating_sub(start + leading),
                    ),
                    call_id,
                })
                .collect();
            ReplyStep {
                reasoning: section.body,
                answer: answer.to_owned(),
                tools,
            }
        })
        .filter(|step| {
            !step.reasoning.is_empty() || !step.answer.is_empty() || !step.tools.is_empty()
        })
        .collect()
}

/// Moves the step anchors and tool marks from the streamed text to the saved
/// reply text. The saved text can differ: it may join the steps' answers
/// trimmed, or drop the answers written before a steering note. Each step's
/// answer is looked up in order; a step whose answer is no longer there gets
/// an empty answer.
pub fn remap_step_anchors(reasoning: &str, streamed: &str, saved: &str) -> String {
    if streamed == saved || !reasoning.contains(REASONING_STEP_ANCHOR) {
        return reasoning.to_owned();
    }
    let sections = parse(reasoning);
    let mut starts = Vec::with_capacity(sections.len());
    let mut last = 0;
    for section in &sections {
        last = section.anchor.unwrap_or(last).min(streamed.len()).max(last);
        starts.push(last);
    }
    let mut cursor = 0;
    let mut out = String::with_capacity(reasoning.len());
    for (index, section) in sections.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(streamed.len());
        let raw = streamed.get(starts[index]..end).unwrap_or_default();
        let leading = raw.len() - raw.trim_start().len();
        let piece = raw.trim();
        let (start, found) = if piece.is_empty() {
            (cursor, false)
        } else if let Some(found) = saved[cursor..].find(piece) {
            let start = cursor + found;
            cursor = start + piece.len();
            (start, true)
        } else {
            (cursor, false)
        };
        if index == 0 {
            out.push_str(&section.body);
        } else {
            out.push_str(&step_start(start));
            out.push_str(&section.body);
        }
        for (offset, call_id) in &section.tools {
            let local = offset
                .saturating_sub(starts[index] + leading)
                .min(piece.len());
            let mapped = if found { start + local } else { cursor };
            out.push_str(&tool_mark(mapped, call_id));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(reasoning: &str, text: &str) -> Vec<(String, String)> {
        reply_steps(reasoning, text)
            .into_iter()
            .map(|step| (step.reasoning, step.answer))
            .collect()
    }

    #[test]
    fn each_step_keeps_its_own_answer() {
        let text = "Here is your answer.Actually, here is your answer.";
        let reasoning = format!("First thought{}Second thought", step_start(20));
        assert_eq!(
            steps(&reasoning, text),
            [
                ("First thought".into(), "Here is your answer.".into()),
                (
                    "Second thought".into(),
                    "Actually, here is your answer.".into()
                ),
            ]
        );
    }

    #[test]
    fn a_step_without_thinking_still_gets_its_own_block() {
        let text = "One.Two.";
        let reasoning = format!("Plan{}", step_start(4));
        assert_eq!(
            steps(&reasoning, text),
            [
                ("Plan".into(), "One.".into()),
                (String::new(), "Two.".into())
            ]
        );
    }

    #[test]
    fn older_reasoning_keeps_the_old_shape() {
        let reasoning = format!("A{REASONING_STEP_BREAK}B");
        assert_eq!(
            steps(&reasoning, " Answer "),
            [("A\n\nB".into(), "Answer".into())]
        );
        assert_eq!(steps("", "Answer"), [(String::new(), "Answer".into())]);
    }

    #[test]
    fn anchors_follow_the_saved_text() {
        let streamed = "Checking.  Done here.";
        let reasoning = format!("Look{}Finish", step_start(11));
        // The saved reply joins the trimmed answers with a blank line.
        let saved = "Checking.\n\nDone here.";
        let remapped = remap_step_anchors(&reasoning, streamed, saved);
        assert_eq!(
            steps(&remapped, saved),
            [
                ("Look".into(), "Checking.".into()),
                ("Finish".into(), "Done here.".into())
            ]
        );
        // A steering note dropped the first answer from the saved reply.
        let remapped = remap_step_anchors(&reasoning, streamed, "Done here.");
        assert_eq!(
            steps(&remapped, "Done here."),
            [
                ("Look".into(), String::new()),
                ("Finish".into(), "Done here.".into())
            ]
        );
    }

    #[test]
    fn plain_reasoning_drops_the_marks() {
        let reasoning = format!("A{}B", step_start(3));
        assert_eq!(
            plain_reasoning(&reasoning),
            "A

B"
        );
        assert_eq!(
            plain_reasoning(&format!("{}more", step_start(9))),
            "

more"
        );
        assert_eq!(plain_reasoning("just text"), "just text");
    }

    #[test]
    fn tools_sit_where_they_ran() {
        let text = "I'll take a look.Here is your answer.";
        let reasoning = format!(
            "First{}{}{}Second{}{}Third",
            tool_mark(0, "a"),
            tool_mark(17, "b"),
            step_start(17),
            tool_mark(17, "c"),
            step_start(17),
        );
        let parts = reply_steps(&reasoning, text);
        let shape = parts
            .iter()
            .map(|step| {
                (
                    step.reasoning.as_str(),
                    step.answer.as_str(),
                    step.tools
                        .iter()
                        .map(|tool| (tool.offset, tool.call_id.as_str()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            shape,
            [
                ("First", "I'll take a look.", vec![(0, "a"), (17, "b")]),
                ("Second", "", vec![(0, "c")]),
                ("Third", "Here is your answer.", vec![]),
            ]
        );
        assert_eq!(plain_reasoning(&reasoning), "First\n\nSecond\n\nThird");
    }

    #[test]
    fn a_reply_with_only_tools_still_places_them() {
        let reasoning = format!("Look{}", tool_mark(0, "a"));
        let parts = reply_steps(&reasoning, "");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].tools[0].call_id, "a");
    }

    #[test]
    fn tool_marks_follow_the_saved_text() {
        let streamed = "Checking.  Done here.";
        let reasoning = format!("Look{}{}Finish", tool_mark(11, "a"), step_start(11));
        let saved = "Checking.\n\nDone here.";
        let remapped = remap_step_anchors(&reasoning, streamed, saved);
        let parts = reply_steps(&remapped, saved);
        assert_eq!(parts[0].answer, "Checking.");
        assert_eq!(parts[0].tools[0].offset, "Checking.".len());
        assert_eq!(parts[1].answer, "Done here.");
    }

    #[test]
    fn hostile_anchors_stay_inside_the_text() {
        let reasoning = format!("A{}B{}C", step_start(999), step_start(1));
        let parts = steps(&reasoning, "héllo");
        let marked = format!("A{}{}", tool_mark(2, "x"), tool_mark(99, "y"));
        for step in reply_steps(&marked, "héllo") {
            for tool in step.tools {
                assert!(step.answer.is_char_boundary(tool.offset));
            }
        }
        assert_eq!(
            parts.iter().map(|(_, a)| a.as_str()).collect::<String>(),
            "héllo"
        );
    }
}
