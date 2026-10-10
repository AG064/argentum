use super::*;
use std::cell::RefCell;

pub(super) const CACHE_BYTES: usize = 512 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Anchor {
    key: (TimelineKind, RunId),
    offset: usize,
}
#[derive(Default)]
pub(super) struct Viewport {
    pub anchor: Option<Anchor>,
    pub updates: u16,
    pub older: bool,
    pub newer: bool,
    pub notice: String,
    pub revision: u64,
    pub drop_newer: bool,
    pending_scroll: i32,
    cache: RefCell<Option<Cached>>,
}
struct Cached {
    revision: u64,
    anchor: Option<Anchor>,
    width: usize,
    height: usize,
    rows: Vec<String>,
}

impl TimelineEntry {
    pub(super) fn key(&self) -> (TimelineKind, RunId) {
        (
            self.kind,
            if matches!(self.kind, TimelineKind::User | TimelineKind::Assistant) {
                self.run_id.unwrap_or(self.view_id)
            } else {
                self.tool_call_id.unwrap_or(self.view_id)
            },
        )
    }
    fn display(&self) -> String {
        let role = match self.kind {
            TimelineKind::User => "you",
            TimelineKind::Assistant => "argentum",
            TimelineKind::Tool => "tool",
            TimelineKind::Approval => "approval",
            TimelineKind::System => "info",
        };
        let mut text = format!("{role}: {}", self.text);
        if !self.detail.is_empty() {
            text.push_str("\n  ");
            text.push_str(&self.detail);
        }
        if self.kind == TimelineKind::Tool && !self.state.is_empty() {
            text.push_str(&format!("\n  result: {}", self.state));
        } else if self.kind == TimelineKind::Assistant
            && matches!(self.state.as_str(), "failed" | "cancelled" | "interrupted")
        {
            text.push_str(&format!("\n  response {}", self.state));
        }
        text
    }
}

impl TuiState {
    pub(super) fn replace_messages_preserving_tools(&mut self, entries: Vec<TimelineEntry>) {
        // Saved message snapshots do not contain the tool results observed in
        // this session. Keep them only for runs included in the refreshed page.
        let mut tools_by_run: BTreeMap<RunId, Vec<TimelineEntry>> = BTreeMap::new();
        for entry in self
            .timeline
            .iter()
            .filter(|entry| entry.kind == TimelineKind::Tool)
        {
            if let Some(run_id) = entry.run_id {
                tools_by_run.entry(run_id).or_default().push(entry.clone());
            }
        }
        let mut refreshed = Vec::new();
        for entry in entries {
            let mut tools = entry
                .run_id
                .and_then(|id| tools_by_run.remove(&id))
                .unwrap_or_default();
            if entry.kind == TimelineKind::User {
                refreshed.push(entry);
                refreshed.append(&mut tools);
            } else {
                refreshed.append(&mut tools);
                refreshed.push(entry);
            }
        }
        self.timeline = refreshed;
    }

    pub(super) fn transcript_changed(&mut self) {
        self.viewport.revision = self.viewport.revision.wrapping_add(1);
        if self.viewport.anchor.is_some() {
            self.viewport.updates = self.viewport.updates.saturating_add(1).min(999);
        }
    }
    fn entry_rows(entry: &TimelineEntry, width: usize) -> Vec<(usize, String)> {
        screen::wrap_positions(&entry.display(), width)
    }
    pub(super) fn transcript_rows(&self, width: usize, height: usize) -> Vec<String> {
        if let Some(cache) = self.viewport.cache.borrow().as_ref() {
            if cache.revision == self.viewport.revision
                && cache.anchor == self.viewport.anchor
                && cache.width == width
                && cache.height == height
            {
                return cache.rows.clone();
            }
        }
        let mut rows = Vec::new();
        if let Some(anchor) = &self.viewport.anchor {
            if let Some(start) = self
                .timeline
                .iter()
                .position(|entry| entry.key() == anchor.key)
            {
                for (index, entry) in self.timeline.iter().enumerate().skip(start) {
                    let wrapped = Self::entry_rows(entry, width);
                    let from = if index == start {
                        wrapped
                            .iter()
                            .rposition(|(offset, _)| *offset <= anchor.offset)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    rows.extend(
                        wrapped
                            .into_iter()
                            .skip(from)
                            .take(height.saturating_sub(rows.len()))
                            .map(|(_, text)| text),
                    );
                    if rows.len() >= height {
                        break;
                    }
                }
            } else {
                rows.push("Viewed range was evicted; Ctrl+End returns to latest.".into());
            }
        } else {
            for entry in self.timeline.iter().rev() {
                rows.extend(
                    Self::entry_rows(entry, width)
                        .into_iter()
                        .rev()
                        .take(height.saturating_sub(rows.len()))
                        .map(|(_, text)| text),
                );
                if rows.len() >= height {
                    break;
                }
            }
            rows.reverse();
        }
        rows.resize(height, String::new());
        *self.viewport.cache.borrow_mut() = Some(Cached {
            revision: self.viewport.revision,
            anchor: self.viewport.anchor.clone(),
            width,
            height,
            rows: rows.clone(),
        });
        rows
    }
    pub(super) fn scroll_rows(&mut self, amount: i32) -> TuiAction {
        if self.timeline.is_empty() {
            return TuiAction::Redraw;
        }
        let (width, height) = self
            .viewport
            .cache
            .borrow()
            .as_ref()
            .map(|c| (c.width, c.height))
            .unwrap_or((
                usize::from(self.terminal_width.saturating_sub(1).max(1)),
                16,
            ));
        let (mut index, mut row) = if let Some(anchor) = &self.viewport.anchor {
            let index = self
                .timeline
                .iter()
                .position(|e| e.key() == anchor.key)
                .unwrap_or(0);
            let rows = Self::entry_rows(&self.timeline[index], width);
            (
                index,
                rows.iter()
                    .rposition(|(offset, _)| *offset <= anchor.offset)
                    .unwrap_or(0),
            )
        } else {
            // Locate the first currently visible row, not the mutable bottom row.
            let mut index = self.timeline.len() - 1;
            let mut remaining = height;
            loop {
                let count = Self::entry_rows(&self.timeline[index], width).len();
                if remaining <= count || index == 0 {
                    break (index, count.saturating_sub(remaining));
                }
                remaining -= count;
                index -= 1;
            }
        };
        let mut steps = amount.unsigned_abs() as usize;
        while steps > 0 {
            if amount < 0 {
                if row > 0 {
                    row -= 1;
                } else if index > 0 {
                    index -= 1;
                    row = Self::entry_rows(&self.timeline[index], width)
                        .len()
                        .saturating_sub(1);
                } else {
                    break;
                }
            } else {
                if row + 1 < Self::entry_rows(&self.timeline[index], width).len() {
                    row += 1;
                } else if index + 1 < self.timeline.len() {
                    index += 1;
                    row = 0;
                } else {
                    break;
                }
            }
            steps -= 1;
        }
        let offset = Self::entry_rows(&self.timeline[index], width)
            .get(row)
            .map_or(0, |(offset, _)| *offset);
        self.viewport.anchor = Some(Anchor {
            key: self.timeline[index].key(),
            offset,
        });
        if steps > 0
            && !self.local_text_mode
            && ((amount < 0 && self.viewport.older) || (amount > 0 && self.viewport.newer))
        {
            self.viewport.pending_scroll = if amount < 0 {
                -(steps as i32)
            } else {
                steps as i32
            };
            let cursor = if amount < 0 {
                self.timeline.iter().find_map(|e| e.message_id)
            } else {
                self.timeline.iter().rev().find_map(|e| e.message_id)
            };
            return TuiAction::HistoryPage {
                cursor,
                newer: amount > 0,
                latest: false,
            };
        }
        if amount > 0 && !self.viewport.newer {
            let below: usize = self
                .timeline
                .iter()
                .skip(index)
                .enumerate()
                .map(|(i, e)| {
                    Self::entry_rows(e, width)
                        .len()
                        .saturating_sub(if i == 0 { row } else { 0 })
                })
                .sum();
            if below <= height {
                self.viewport.anchor = None;
                self.viewport.updates = 0;
            }
        }
        TuiAction::Redraw
    }
    pub(super) fn jump_latest(&mut self) -> TuiAction {
        self.viewport.anchor = None;
        self.viewport.updates = 0;
        if self.viewport.newer && !self.local_text_mode {
            TuiAction::HistoryPage {
                cursor: None,
                newer: false,
                latest: true,
            }
        } else {
            TuiAction::Redraw
        }
    }
    pub(super) fn apply_history_page(
        &mut self,
        page: argentum_store::HistoryPage,
        newer: bool,
        latest: bool,
    ) {
        if Some(page.project_id) != self.project_id
            || Some(page.session_id) != self.active_session_id
        {
            return;
        }
        let mut entries: Vec<_> = page
            .messages
            .into_iter()
            .map(|message| TimelineEntry {
                view_id: message.id,
                message_id: Some(message.id),
                kind: if message.role == ConversationRole::User {
                    TimelineKind::User
                } else {
                    TimelineKind::Assistant
                },
                run_id: Some(message.run_id),
                tool_call_id: None,
                text: message.text,
                detail: if message.truncated {
                    "Message exceeds the display limit; remainder unavailable in this view.".into()
                } else {
                    String::new()
                },
                state: format!("{:?}", message.status).to_lowercase(),
            })
            .collect();
        if latest {
            self.replace_messages_preserving_tools(entries);
            self.merge_current_streams();
            self.viewport.anchor = None;
            self.viewport.older = page.has_older;
            self.viewport.newer = page.has_newer;
        } else {
            entries.retain(|entry| !self.timeline.iter().any(|old| old.key() == entry.key()));
            if newer {
                self.timeline.extend(entries);
                self.viewport.newer = page.has_newer;
            } else {
                entries.append(&mut self.timeline);
                self.timeline = entries;
                self.viewport.older = page.has_older;
            }
        }
        self.viewport.drop_newer = !newer && !latest;
        self.truncate_timeline();
        self.viewport.revision = self.viewport.revision.wrapping_add(1);
        let pending = std::mem::take(&mut self.viewport.pending_scroll);
        if pending != 0 {
            let _ = self.scroll_rows(pending);
        }
    }
}
