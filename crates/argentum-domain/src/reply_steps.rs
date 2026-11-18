// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! How a reply's shown reasoning records its model steps, so the chat can
//! show each step's thinking followed by the answer text that step wrote.
//!
//! The shown reasoning is split into steps by [`REASONING_STEP_MARK`]. A step
//! after the first starts with [`REASONING_STEP_ANCHOR`], the byte offset in
//! the reply text where that step's answer begins, and a line break. Only the
//! shown copy carries these marks; nothing here is sent to a provider.

/// Separates one model step's reasoning from the next.
pub const REASONING_STEP_BREAK: &str = "\n\n\u{2063}\n\n";
pub const REASONING_STEP_MARK: char = '\u{2063}';
/// Starts a step's anchor: the reply text offset where the step's answer begins.
pub const REASONING_STEP_ANCHOR: char = '\u{2064}';

/// The text that opens a new step whose answer begins at `offset`.
pub fn step_start(offset: usize) -> String {
    format!("{REASONING_STEP_BREAK}{REASONING_STEP_ANCHOR}{offset}\n")
}

/// The shown reasoning as plain text, for views that do not split a reply
/// into steps: each step's thinking, separated by a blank line.
pub fn plain_reasoning(reasoning: &str) -> String {
    let plain = parse(reasoning)
        .into_iter()
        .map(|(_, body)| body)
        .filter(|body| !body.is_empty())
        .collect::<Vec<_>>()
        .join(
            "

",
        );
    // A streamed piece that opens a new step keeps the gap before it.
    if reasoning.trim_start().starts_with(REASONING_STEP_MARK) {
        format!(
            "

{plain}"
        )
    } else {
        plain
    }
}

/// One model step of a reply: what it thought, then what it answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyStep {
    pub reasoning: String,
    pub answer: String,
}

fn parse(reasoning: &str) -> Vec<(Option<usize>, String)> {
    reasoning
        .split(REASONING_STEP_MARK)
        .map(|section| {
            let section = section.trim_start_matches(['\n', '\r', ' ']);
            if let Some(rest) = section.strip_prefix(REASONING_STEP_ANCHOR) {
                let (number, body) = rest.split_once('\n').unwrap_or((rest, ""));
                (number.trim().parse().ok(), body.trim().to_owned())
            } else {
                (None, section.trim().to_owned())
            }
        })
        .collect()
}

/// The reply as its model steps, in order. Reasoning stored before steps had
/// anchors gives one step with all of its reasoning, then the whole answer.
pub fn reply_steps(reasoning: &str, text: &str) -> Vec<ReplyStep> {
    let sections = parse(reasoning);
    if sections.iter().all(|(anchor, _)| anchor.is_none()) {
        let reasoning = sections
            .into_iter()
            .map(|(_, body)| body)
            .filter(|body| !body.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        return vec![ReplyStep {
            reasoning,
            answer: text.trim().to_owned(),
        }];
    }
    let boundary = |offset: usize| {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    let mut starts = Vec::with_capacity(sections.len());
    let mut last = 0;
    for (anchor, _) in &sections {
        last = anchor.map_or(last, |offset| boundary(offset).max(last));
        starts.push(last);
    }
    sections
        .into_iter()
        .enumerate()
        .map(|(index, (_, reasoning))| {
            let end = starts.get(index + 1).copied().unwrap_or(text.len());
            ReplyStep {
                reasoning,
                answer: text[starts[index]..end].trim().to_owned(),
            }
        })
        .filter(|step| !step.reasoning.is_empty() || !step.answer.is_empty())
        .collect()
}

/// Moves the step anchors from the streamed text to the saved reply text.
/// The saved text can differ: it may join the steps' answers trimmed, or drop
/// the answers written before a steering note. Each step's answer is looked up
/// in order; a step whose answer is no longer there gets an empty answer.
pub fn remap_step_anchors(reasoning: &str, streamed: &str, saved: &str) -> String {
    if streamed == saved || !reasoning.contains(REASONING_STEP_ANCHOR) {
        return reasoning.to_owned();
    }
    let sections = parse(reasoning);
    let mut starts = Vec::with_capacity(sections.len());
    let mut last = 0;
    for (anchor, _) in &sections {
        last = anchor.unwrap_or(last).min(streamed.len()).max(last);
        starts.push(last);
    }
    let mut cursor = 0;
    let mut out = String::with_capacity(reasoning.len());
    for (index, (_, body)) in sections.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(streamed.len());
        let piece = streamed
            .get(starts[index]..end)
            .map(str::trim)
            .unwrap_or_default();
        let start = if piece.is_empty() {
            cursor
        } else if let Some(found) = saved[cursor..].find(piece) {
            let start = cursor + found;
            cursor = start + piece.len();
            start
        } else {
            cursor
        };
        if index == 0 {
            out.push_str(body);
        } else {
            out.push_str(&step_start(start));
            out.push_str(body);
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
    fn hostile_anchors_stay_inside_the_text() {
        let reasoning = format!("A{}B{}C", step_start(999), step_start(1));
        let parts = steps(&reasoning, "héllo");
        assert_eq!(
            parts.iter().map(|(_, a)| a.as_str()).collect::<String>(),
            "héllo"
        );
    }
}
