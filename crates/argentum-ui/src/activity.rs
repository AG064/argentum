// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! What a run did, in words: "Read 3 files, ran 2 commands" and the files it changed.
//!
//! Kept for the running session only. Command text is never stored, so a restart
//! shows the recorded answer without these details.

use std::collections::BTreeMap;

use argentum_domain::{FileChangeRecord, ToolCallId, ToolResultState, ToolTrace};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChangedFile {
    /// The latest recorded change, which is the one an opened diff shows.
    pub id: String,
    pub additions: u32,
    pub removals: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RunActivity {
    described: BTreeMap<ToolCallId, String>,
    lines: Vec<String>,
    reads: u32,
    lists: u32,
    searches: u32,
    commands: u32,
    git: u32,
    others: u32,
    failures: u32,
    pub files: BTreeMap<String, ChangedFile>,
}

fn counted(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        one.to_owned()
    } else {
        format!("{count} {many}")
    }
}

impl RunActivity {
    pub fn describe(&mut self, call: ToolCallId, text: &str) {
        self.described.insert(call, text.to_owned());
    }

    pub fn finish(&mut self, trace: &ToolTrace, fallback: &str) {
        let text = self
            .described
            .remove(&trace.id)
            .unwrap_or_else(|| fallback.to_owned());
        match trace.result {
            ToolResultState::Succeeded => self.lines.push(text),
            ToolResultState::Cancelled => self.lines.push(format!("{text} (cancelled)")),
            ToolResultState::Running => {}
            ToolResultState::Failed | ToolResultState::NotExecuted => {
                self.failures += 1;
                self.lines.push(format!("{text} (failed)"));
            }
        }
        if !matches!(trace.result, ToolResultState::Succeeded) {
            return;
        }
        match trace.tool_id.as_str() {
            "read_text" | "file_metadata" | "read_binary_metadata" => self.reads += 1,
            "list_directory" => self.lists += 1,
            "search_text" | "search_filenames" => self.searches += 1,
            "execute_shell" | "execute_argv" => self.commands += 1,
            "git_read" | "git_write" => self.git += 1,
            "write_text" | "edit_text" | "apply_patch" | "move_file" | "copy_file"
            | "delete_file" | "create_directory" => {}
            _ => self.others += 1,
        }
    }

    pub fn record_change(&mut self, record: &FileChangeRecord) {
        let file = self.files.entry(record.relative_path.clone()).or_default();
        file.id = record.id.to_string();
        file.additions = file.additions.saturating_add(record.additions);
        file.removals = file.removals.saturating_add(record.removals);
    }

    pub fn details(&self) -> String {
        self.lines.join("\n")
    }

    pub fn additions(&self) -> u32 {
        self.files.values().map(|file| file.additions).sum()
    }

    pub fn removals(&self) -> u32 {
        self.files.values().map(|file| file.removals).sum()
    }

    /// The one line shown while the details are closed.
    pub fn title(&self) -> String {
        let mut parts = Vec::new();
        if self.files.len() == 1 {
            if let Some(path) = self.files.keys().next() {
                let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                parts.push(format!("Changed {name}"));
            }
        } else if self.files.len() > 1 {
            parts.push(format!("Changed {} files", self.files.len()));
        }
        if self.reads > 0 {
            parts.push(format!("read {}", counted(self.reads, "a file", "files")));
        }
        if self.lists > 0 {
            parts.push(format!(
                "listed {}",
                counted(self.lists, "a folder", "folders")
            ));
        }
        if self.searches > 0 {
            parts.push(format!(
                "searched {}",
                if self.searches == 1 {
                    "once".to_owned()
                } else {
                    format!("{} times", self.searches)
                }
            ));
        }
        if self.commands > 0 {
            parts.push(format!(
                "ran {}",
                counted(self.commands, "a command", "commands")
            ));
        }
        if self.git > 0 {
            parts.push(format!("used Git {}", counted(self.git, "once", "times")));
        }
        if self.others > 0 {
            parts.push(format!("used {}", counted(self.others, "a tool", "tools")));
        }
        if self.failures > 0 {
            parts.push(format!("{} failed", self.failures));
        }
        let Some(first) = parts.first_mut() else {
            return String::new();
        };
        if let Some(letter) = first.get(..1).map(str::to_ascii_uppercase) {
            first.replace_range(..1, &letter);
        }
        parts.join(", ")
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
}
