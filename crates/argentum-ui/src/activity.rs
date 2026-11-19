// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! What a run did, in words: "Read 3 files, ran 2 commands" and the files it changed.
//!
//! Kept for the running session only. Command text is never stored, so a restart
//! shows the recorded answer without these details. A reply's shown reasoning
//! records where each call ran, and the chat shows each call's group there.

use std::collections::{BTreeMap, HashSet};

use argentum_domain::{FileChangeRecord, ToolCallId, ToolResultState, ToolTrace};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChangedFile {
    /// The latest recorded change, which is the one an opened diff shows.
    pub id: String,
    pub additions: u32,
    pub removals: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Read,
    List,
    Search,
    Command,
    Git,
    Edit,
    Other,
    /// Cancelled, failed or not run: counted only as a failure, if at all.
    Unfinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    id: ToolCallId,
    line: String,
    kind: Kind,
    failed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RunActivity {
    described: BTreeMap<ToolCallId, String>,
    calls: Vec<Call>,
    pub files: BTreeMap<String, ChangedFile>,
}

/// A run of tool calls shown together at one point of a reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolGroup {
    pub title: String,
    pub details: String,
    /// A call in the group is still running.
    pub running: bool,
}

fn counted(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        one.to_owned()
    } else {
        format!("{count} {many}")
    }
}

fn capitalized(mut text: String) -> String {
    if let Some(letter) = text.get(..1).map(str::to_ascii_uppercase) {
        text.replace_range(..1, &letter);
    }
    text
}

/// What a set of calls did, in one line. `changed` names the files the run
/// changed, for the line of the whole run; a group counts its edits instead.
fn summary<'a>(
    calls: impl Iterator<Item = &'a Call>,
    changed: Option<&BTreeMap<String, ChangedFile>>,
) -> String {
    let mut counts = [0_u32; 7];
    let mut failures = 0_u32;
    for call in calls {
        if call.failed {
            failures += 1;
        }
        let slot = match call.kind {
            Kind::Read => 0,
            Kind::List => 1,
            Kind::Search => 2,
            Kind::Command => 3,
            Kind::Git => 4,
            Kind::Edit => 5,
            Kind::Other => 6,
            Kind::Unfinished => continue,
        };
        counts[slot] += 1;
    }
    let [reads, lists, searches, commands, git, edits, others] = counts;
    let mut parts = Vec::new();
    match changed {
        Some(files) if files.len() == 1 => {
            if let Some(path) = files.keys().next() {
                let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                parts.push(format!("Changed {name}"));
            }
        }
        Some(files) if files.len() > 1 => parts.push(format!("Changed {} files", files.len())),
        _ if edits > 0 => parts.push(format!("made {}", counted(edits, "an edit", "edits"))),
        _ => {}
    }
    if reads > 0 {
        parts.push(format!("read {}", counted(reads, "a file", "files")));
    }
    if lists > 0 {
        parts.push(format!("listed {}", counted(lists, "a folder", "folders")));
    }
    if searches > 0 {
        parts.push(format!(
            "searched {}",
            if searches == 1 {
                "once".to_owned()
            } else {
                format!("{searches} times")
            }
        ));
    }
    if commands > 0 {
        parts.push(format!(
            "ran {}",
            counted(commands, "a command", "commands")
        ));
    }
    if git > 0 {
        parts.push(format!("used Git {}", counted(git, "once", "times")));
    }
    if others > 0 {
        parts.push(format!("used {}", counted(others, "a tool", "tools")));
    }
    if failures > 0 {
        parts.push(format!("{failures} failed"));
    }
    if parts.is_empty() {
        return String::new();
    }
    capitalized(parts.join(", "))
}

impl RunActivity {
    pub fn describe(&mut self, call: ToolCallId, text: &str) {
        self.described.insert(call, text.to_owned());
    }

    pub fn finish(&mut self, trace: &ToolTrace, fallback: &str) {
        if matches!(trace.result, ToolResultState::Running) {
            return;
        }
        let text = self
            .described
            .remove(&trace.id)
            .unwrap_or_else(|| fallback.to_owned());
        let (line, failed) = match trace.result {
            ToolResultState::Cancelled => (format!("{text} (cancelled)"), false),
            ToolResultState::Failed | ToolResultState::NotExecuted => {
                (format!("{text} (failed)"), true)
            }
            _ => (text, false),
        };
        let kind = if !matches!(trace.result, ToolResultState::Succeeded) {
            Kind::Unfinished
        } else {
            match trace.tool_id.as_str() {
                "read_text" | "file_metadata" | "read_binary_metadata" => Kind::Read,
                "list_directory" => Kind::List,
                "search_text" | "search_filenames" => Kind::Search,
                "execute_shell" | "execute_argv" => Kind::Command,
                "git_read" | "git_write" => Kind::Git,
                "write_text" | "edit_text" | "apply_patch" | "move_file" | "copy_file"
                | "delete_file" | "create_directory" => Kind::Edit,
                _ => Kind::Other,
            }
        };
        self.calls.retain(|call| call.id != trace.id);
        self.calls.push(Call {
            id: trace.id,
            line,
            kind,
            failed,
        });
    }

    pub fn record_change(&mut self, record: &FileChangeRecord) {
        let file = self.files.entry(record.relative_path.clone()).or_default();
        file.id = record.id.to_string();
        file.additions = file.additions.saturating_add(record.additions);
        file.removals = file.removals.saturating_add(record.removals);
    }

    /// The calls a reply does not place at a point of its own, one a line.
    pub fn details_except(&self, placed: &HashSet<String>) -> String {
        self.calls
            .iter()
            .filter(|call| !placed.contains(&call.id.to_string()))
            .map(|call| call.line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[cfg(test)]
    pub fn details(&self) -> String {
        self.details_except(&HashSet::new())
    }

    pub fn additions(&self) -> u32 {
        self.files.values().map(|file| file.additions).sum()
    }

    pub fn removals(&self) -> u32 {
        self.files.values().map(|file| file.removals).sum()
    }

    /// The one line shown while the details are closed, for the calls a
    /// reply does not place, and the files the run changed.
    pub fn title_except(&self, placed: &HashSet<String>) -> String {
        let shown = self
            .calls
            .iter()
            .filter(|call| !placed.contains(&call.id.to_string()));
        let files = if placed.is_empty() || shown.clone().next().is_some() {
            Some(&self.files)
        } else {
            None
        };
        summary(shown, files)
    }

    #[cfg(test)]
    pub fn title(&self) -> String {
        self.title_except(&HashSet::new())
    }

    /// The calls with these ids, in the order given: finished ones by what
    /// they did, running ones by what they are doing.
    pub fn group(&self, ids: &[&str]) -> ToolGroup {
        let mut calls = Vec::new();
        let mut lines = Vec::new();
        let mut running = Vec::new();
        for id in ids {
            if let Some(call) = self.calls.iter().find(|call| call.id.to_string() == *id) {
                lines.push(call.line.clone());
                calls.push(call);
            } else if let Some((_, text)) = self
                .described
                .iter()
                .find(|(call, _)| call.to_string() == *id)
            {
                lines.push(text.clone());
                running.push(text.clone());
            }
        }
        let title = match (summary(calls.into_iter(), None), running.first()) {
            (done, Some(now)) if done.is_empty() => now.clone(),
            (done, Some(_)) => format!("{done}, working"),
            (done, None) => done,
        };
        ToolGroup {
            title,
            details: lines.join("\n"),
            running: !running.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::RunId;

    fn trace(tool: &str, result: ToolResultState) -> ToolTrace {
        ToolTrace {
            id: ToolCallId::new_v4(),
            run_id: RunId::new_v4(),
            tool_id: tool.into(),
            summary: "audit text".into(),
            result,
            duration_ms: None,
        }
    }

    #[test]
    fn the_title_says_what_the_run_did() {
        let mut activity = RunActivity::default();
        assert_eq!(activity.title(), "");
        for _ in 0..3 {
            activity.finish(&trace("read_text", ToolResultState::Succeeded), "x");
        }
        assert_eq!(activity.title(), "Read 3 files");
        activity.finish(&trace("execute_shell", ToolResultState::Succeeded), "x");
        assert_eq!(activity.title(), "Read 3 files, ran a command");
        activity.finish(&trace("execute_shell", ToolResultState::Failed), "x");
        assert_eq!(activity.title(), "Read 3 files, ran a command, 1 failed");
    }

    #[test]
    fn details_use_the_description_and_mark_failures() {
        let mut activity = RunActivity::default();
        let done = trace("execute_shell", ToolResultState::Succeeded);
        activity.describe(done.id, "Ran `cargo test`");
        activity.finish(&done, "fallback");
        activity.finish(&trace("read_text", ToolResultState::Failed), "Read file");
        assert_eq!(activity.details(), "Ran `cargo test`\nRead file (failed)");
    }

    #[test]
    fn groups_say_what_their_own_calls_did() {
        let mut activity = RunActivity::default();
        let first = trace("read_text", ToolResultState::Succeeded);
        let second = trace("read_text", ToolResultState::Succeeded);
        let edit = trace("edit_text", ToolResultState::Succeeded);
        activity.describe(first.id, "Read `a.rs`");
        activity.finish(&first, "x");
        activity.finish(&second, "Read file");
        activity.finish(&edit, "Edited file");
        let running = ToolCallId::new_v4();
        activity.describe(running, "Running `cargo test`");
        let ids = [first.id.to_string(), second.id.to_string()];
        let group = activity.group(&ids.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(group.title, "Read 2 files");
        assert_eq!(group.details, "Read `a.rs`\nRead file");
        assert!(!group.running);
        let ids = [edit.id.to_string(), running.to_string()];
        let group = activity.group(&ids.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(group.title, "Made an edit, working");
        assert!(group.running);
        let placed = [first.id, second.id, edit.id]
            .iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();
        assert_eq!(activity.details_except(&placed), "");
        assert_eq!(activity.title_except(&placed), "");
    }
}
