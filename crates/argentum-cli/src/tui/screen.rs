use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::style::{cells, Row, Span, Tone};
use super::TuiState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub rows: Vec<String>,
    pub styles: Vec<Vec<Span>>,
    pub cursor: (u16, u16),
}

const PLACEHOLDER: &str = "Ask anything, or type / for commands";
const APPROVAL_PLACEHOLDER: &str = "Type y to approve once or n to reject, then Enter";

fn elapsed(state: &TuiState) -> String {
    let active = state.progress_active();
    let mut since = state.progress_since.get();
    if active && since.is_none() {
        since = Some(std::time::Instant::now());
    } else if !active {
        since = None;
    }
    state.progress_since.set(since);
    let seconds = since.map_or(0, |at| at.elapsed().as_secs());
    if seconds >= 60 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

// The pulse uses glyphs that exist in the default Windows console fonts.
fn status_row(state: &TuiState) -> Option<Row> {
    if state.progress_active() {
        let pulse =
            ["\u{00b7}", "\u{2022}", "\u{25cf}", "\u{2022}"][usize::from(state.progress_tick % 4)];
        let mut row = Row::default();
        row.push(pulse, Tone::Accent);
        row.push(" ", Tone::Plain);
        row.push(&state.status, Tone::Strong);
        let stop = if state.active_run_id.is_some() || state.manual_compaction.is_some() {
            " \u{00b7} Esc to interrupt"
        } else {
            ""
        };
        row.push(&format!("  {}{stop}", elapsed(state)), Tone::Muted);
        return Some(row);
    }
    let _ = elapsed(state);
    let repeats_transcript = state
        .timeline
        .last()
        .is_some_and(|entry| entry.text == state.status);
    if state.pending_approval.is_some()
        || state.status.is_empty()
        || matches!(state.status.as_str(), "Ready" | "Completed")
        || repeats_transcript
    {
        return None;
    }
    Some(Row::toned(state.status.clone(), Tone::Muted))
}

fn footer_row(state: &TuiState, hint: &str, columns: usize) -> Row {
    let model = if !state.model_name.is_empty() {
        state.model_name.as_str()
    } else if state.model.is_empty() {
        state.provider_label.as_str()
    } else {
        state.model.as_str()
    };
    let mut left = Row::default();
    left.push(model, Tone::Muted);
    left.push(" \u{00b7} ", Tone::Faint);
    let full_access = state.permission_mode.to_ascii_lowercase().contains("full");
    left.push(
        &state.permission_mode,
        if full_access {
            Tone::Danger
        } else {
            Tone::Muted
        },
    );
    if let Some((session, used, usable)) = state.context_use {
        if Some(session) == state.active_session_id && usable > 0 {
            left.push(" \u{00b7} ", Tone::Faint);
            left.push(
                &format!("{}% context", used.saturating_mul(100) / usable),
                Tone::Muted,
            );
        }
    }
    let room = columns.saturating_sub(cells(&left.text));
    if cells(&left.text) >= columns {
        return Row::toned(wrap(hint, columns).remove(0), Tone::Muted);
    }
    if cells(hint) + 2 > room {
        return left;
    }
    left.push(&" ".repeat(room - cells(hint)), Tone::Plain);
    left.push(hint, Tone::Faint);
    left
}

fn welcome_rows(state: &TuiState, columns: usize) -> Vec<Row> {
    let mut rows = vec![Row::default()];
    let mut title = Row::default();
    title.push("\u{25b2} ", Tone::Accent);
    title.push("Argentum Genesis", Tone::Strong);
    title.push(concat!("  v", env!("CARGO_PKG_VERSION")), Tone::Faint);
    rows.push(title);
    let workspace = state.workspace.display().to_string();
    let workspace = workspace.strip_prefix(r"\\?\").unwrap_or(&workspace);
    rows.push(Row::toned(
        wrap(&format!("  {workspace}"), columns).remove(0),
        Tone::Muted,
    ));
    rows.push(Row::default());
    if let Some(issue) = &state.coding_preflight_issue {
        for line in wrap(issue, columns.saturating_sub(2).max(1)) {
            rows.push(Row::toned(format!("  {line}"), Tone::Danger));
        }
        rows.push(Row::default());
    }
    for (keys, what) in [
        ("/", "commands"),
        ("/model", "switch model"),
        ("/permissions", "change access"),
        ("/changes", "review edits"),
        ("F1", "all shortcuts"),
    ] {
        let mut row = Row::plain("  ");
        row.push(&format!("{keys:<14}"), Tone::Plain);
        row.push(what, Tone::Muted);
        rows.push(row);
    }
    rows
}

fn boxed(top: Option<&str>, tone: Tone, columns: usize) -> Row {
    let inner = columns.saturating_sub(2);
    let mut row = Row::default();
    match top {
        Some(title) => {
            let label = format!(" {title} ");
            row.push("\u{256d}\u{2500}", tone);
            row.push(&label, tone);
            row.push(
                &"\u{2500}".repeat(inner.saturating_sub(1 + cells(&label))),
                tone,
            );
            row.push("\u{256e}", tone);
        }
        None => {
            row.push("\u{2570}", tone);
            row.push(&"\u{2500}".repeat(inner), tone);
            row.push("\u{256f}", tone);
        }
    }
    row
}

fn boxed_text(text: &str, tone: Tone, border: Tone, columns: usize) -> Row {
    let inner = columns.saturating_sub(4);
    let mut row = Row::default();
    row.push("\u{2502} ", border);
    row.push(text, tone);
    row.push(&" ".repeat(inner.saturating_sub(cells(text))), Tone::Plain);
    row.push(" \u{2502}", border);
    row
}

// The host draws this card at column 0, right above the composer. Transcript
// content is always indented, so a model cannot draw a matching prompt.
fn approval_rows(state: &TuiState, columns: usize, limit: usize) -> Vec<Row> {
    let Some(request) = state.pending_approval.as_ref() else {
        return Vec::new();
    };
    if columns < 24 || limit < 4 {
        return Vec::new();
    }
    let inner = columns.saturating_sub(4);
    let mut lines = wrap(&format!("{} on {}", request.action, request.target), inner);
    let reason = wrap(&request.reason, inner);
    lines.truncate(limit.saturating_sub(3).max(1));
    let mut rows = vec![boxed(Some("Permission needed"), Tone::Danger, columns)];
    for line in &lines {
        rows.push(boxed_text(line, Tone::Strong, Tone::Danger, columns));
    }
    if rows.len() + 2 <= limit {
        if let Some(first) = reason.first().filter(|line| !line.is_empty()) {
            rows.push(boxed_text(first, Tone::Muted, Tone::Danger, columns));
        }
    }
    rows.push(boxed(None, Tone::Danger, columns));
    rows
}

// Keep one column unused: writing the last cell can trigger wrapping or scroll
// immediately on Windows. No emitted row depends on the host's newline mode.
pub(super) fn frame(state: &TuiState, width: u16, height: u16) -> Frame {
    let columns = usize::from(width.saturating_sub(1).clamp(1, 512));
    let height = usize::from(height.clamp(1, 200));
    if width <= 1 {
        return Frame {
            rows: vec![String::new(); height],
            styles: vec![Vec::new(); height],
            cursor: (0, (height - 1) as u16),
        };
    }
    let framed = height >= 8 && columns >= 24;
    let (lead, rest, inset) = if framed {
        ("\u{2502} \u{203a} ", "\u{2502}   ", 4)
    } else {
        ("\u{203a} ", "  ", 2)
    };
    let inner = columns
        .saturating_sub(inset + usize::from(framed) * 2)
        .max(1);
    let (composer, cursor) = layout(&state.composer, inner, Some(state.composer_cursor));
    let border_rows = usize::from(framed) * 2;
    let hint_rows = usize::from(height > 1);
    let status = status_row(state).filter(|_| height >= 6);
    let reserved = border_rows + hint_rows + usize::from(status.is_some());
    let composer_space = height
        .saturating_sub(reserved + 1)
        .max(1)
        .min(composer.len());
    let start = cursor.1.saturating_add(1).saturating_sub(composer_space);
    let composer_end = (start + composer_space).min(composer.len());
    let shown = composer_end - start;
    let bottom = shown + reserved;
    let approval = if state.overlay.is_none() {
        approval_rows(state, columns, height.saturating_sub(bottom + 1).min(7))
    } else {
        Vec::new()
    };
    let body_space = height.saturating_sub(bottom + approval.len());

    let mut rows: Vec<Row> = Vec::new();
    if !state.viewport.notice.is_empty() && rows.len() < body_space {
        rows.push(Row::toned(
            wrap(&state.viewport.notice, columns).remove(0),
            Tone::Muted,
        ));
    }
    if let Some(skill) = &state.next_skill {
        if rows.len() < body_space {
            rows.push(Row::toned(
                wrap(&format!("Next message skill: {}", skill.name), columns).remove(0),
                Tone::Muted,
            ));
        }
    }
    let available = body_space.saturating_sub(rows.len());
    let overlay_start = rows.len();
    let settings_cursor = super::settings::cursor(state, columns, available)
        .map(|(x, y)| (x, y.saturating_add(overlay_start as u16)));
    if let Some(overlay) = state.overlay_rows(columns, available) {
        rows.extend(
            overlay
                .into_iter()
                .enumerate()
                .map(|(index, text)| super::style::overlay_row(index, text)),
        );
    } else if state.timeline.is_empty() {
        rows.extend(welcome_rows(state, columns).into_iter().take(available));
    } else {
        rows.extend(state.transcript_styled(columns, available));
    }
    rows.resize(body_space, Row::default());
    rows.extend(approval);
    if let Some(status) = status {
        rows.push(status);
    }

    let empty = state.composer.is_empty();
    let placeholder = if state.pending_approval.is_some() {
        APPROVAL_PLACEHOLDER
    } else {
        PLACEHOLDER
    };
    let border = if state.pending_approval.is_some() {
        Tone::Danger
    } else {
        Tone::Faint
    };
    if framed {
        let mut top = Row::default();
        top.push("\u{256d}", border);
        top.push(&"\u{2500}".repeat(columns.saturating_sub(2)), border);
        top.push("\u{256e}", border);
        rows.push(top);
    }
    let cursor_row = rows.len() + cursor.1 - start;
    for (index, line) in composer[start..composer_end].iter().enumerate() {
        let mut row = Row::default();
        let is_first = index + start == 0;
        let prefix = if is_first { lead } else { rest };
        if framed {
            row.push("\u{2502} ", border);
            row.push(&prefix["\u{2502} ".len()..], Tone::Accent);
        } else {
            row.push(prefix, Tone::Accent);
        }
        let text = if empty && is_first {
            wrap(placeholder, inner).remove(0)
        } else {
            line.clone()
        };
        row.push(&text, if empty { Tone::Faint } else { Tone::Plain });
        if framed {
            row.push(&" ".repeat(inner.saturating_sub(cells(&text))), Tone::Plain);
            row.push(" \u{2502}", border);
        }
        rows.push(row);
    }
    if framed {
        rows.push(boxed(None, border, columns));
    }

    if hint_rows > 0 {
        let hint: String = if state.diagnostic_active {
            "CAPTURE ON: F10 stops. Synthetic only.".into()
        } else if state.interrupt_armed {
            "Press Ctrl+C again to exit.".into()
        } else if state.quit_armed {
            "Repeat /quit to discard, /queue continue to keep working.".into()
        } else if let Some(overlay) = &state.overlay {
            match overlay.view {
                super::menu::View::AccessReview => {
                    "Hold F8 apply | Esc cancel | PgUp/PgDn details".into()
                }
                super::menu::View::Permissions => {
                    "Arrows choose | Enter review | Tab Advanced | Esc cancel".into()
                }
                super::menu::View::Changes => {
                    "Arrows select/scroll | Enter opens | Esc back/close | Typing: draft".into()
                }
                super::menu::View::Commands | super::menu::View::Help => {
                    "Type filter | Arrows choose | Enter open | Esc closes".into()
                }
                _ if overlay.settings.is_some() => {
                    "Arrows choose | Enter opens/applies | Esc back | Ctrl+U clears input".into()
                }
                _ => "PgUp/PgDn or arrows scroll | Esc returns to draft".into(),
            }
        } else if state.viewport.anchor.is_some() {
            format!(
                "Reading | {} new updates | Ctrl+End latest",
                state.viewport.updates
            )
        } else {
            "/ commands  F1 help  F2 access".into()
        };
        rows.push(footer_row(state, &hint, columns));
    }
    rows.resize(height, Row::default());
    for row in &mut rows {
        row.clip(columns);
    }
    let inset_cursor = (cursor.0 + inset).min(usize::from(width.saturating_sub(1)));
    let cursor =
        settings_cursor.unwrap_or((inset_cursor as u16, cursor_row.min(height - 1) as u16));
    let (texts, styles) = rows.into_iter().map(|row| (row.text, row.spans)).unzip();
    Frame {
        rows: texts,
        styles,
        cursor,
    }
}

pub(super) fn wrap(text: &str, width: usize) -> Vec<String> {
    layout(text, width, None).0
}

pub(super) fn layout(
    text: &str,
    width: usize,
    cursor_byte: Option<usize>,
) -> (Vec<String>, (usize, usize)) {
    let width = width.max(1);
    let mut rows = vec![String::new()];
    let mut column = 0;
    let mut cursor = (0, 0);
    for (offset, grapheme) in text.grapheme_indices(true) {
        if column >= width && grapheme != "\n" && grapheme != "\r\n" {
            rows.push(String::new());
            column = 0;
        }
        if cursor_byte == Some(offset) {
            cursor = (column, rows.len() - 1);
        }
        if matches!(grapheme, "\n" | "\r\n") {
            rows.push(String::new());
            column = 0;
            continue;
        }
        let display = if grapheme == "\t" {
            " ".repeat((4 - column % 4).min(width - column))
        } else if grapheme.chars().any(char::is_control) {
            "?".into()
        } else {
            grapheme.to_owned()
        };
        let cells = UnicodeWidthStr::width(display.as_str());
        let (display, cells) = if cells > width {
            ("?".to_owned(), 1)
        } else {
            (display, cells)
        };
        if column + cells > width {
            rows.push(String::new());
            column = 0;
            if cursor_byte == Some(offset) {
                cursor = (0, rows.len() - 1);
            }
        }
        rows.last_mut().unwrap().push_str(&display);
        column += cells;
    }
    if cursor_byte == Some(text.len()) {
        if column >= width {
            rows.push(String::new());
            column = 0;
        }
        cursor = (column, rows.len() - 1);
    }
    (rows, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::{InputEvent, TtyRenderer};
    use std::path::PathBuf;

    // A test-only screen model for the exact output subset emitted here. This
    // is layout evidence, not a terminal-host or physical keyboard test.
    struct Screen {
        cells: Vec<Vec<String>>,
        cursor: (usize, usize),
        scrolls: usize,
    }

    impl Screen {
        fn new(width: usize, height: usize) -> Self {
            Self {
                cells: vec![vec![" ".into(); width]; height],
                cursor: (0, 0),
                scrolls: 0,
            }
        }
        fn down(&mut self) {
            self.cursor.1 += 1;
            if self.cursor.1 == self.cells.len() {
                let width = self.cells[0].len();
                self.cells.remove(0);
                self.cells.push(vec![" ".into(); width]);
                self.cursor.1 -= 1;
                self.scrolls += 1;
            }
        }
        fn feed(&mut self, bytes: &[u8]) {
            let mut text = std::str::from_utf8(bytes).unwrap();
            while !text.is_empty() {
                if let Some(rest) = text.strip_prefix("\x1b[") {
                    let end = rest.find(|c: char| c.is_ascii_alphabetic()).unwrap();
                    let params = &rest[..end];
                    match rest.as_bytes()[end] {
                        b'H' => {
                            let (row, col) = params.split_once(';').unwrap();
                            self.cursor = (
                                col.parse::<usize>().unwrap() - 1,
                                row.parse::<usize>().unwrap() - 1,
                            );
                            assert!(self.cursor.0 < self.cells[0].len());
                            assert!(self.cursor.1 < self.cells.len());
                        }
                        b'J' => {
                            assert_eq!(params, "2");
                            for row in &mut self.cells {
                                row.fill(" ".into());
                            }
                        }
                        b'K' => {
                            assert_eq!(params, "2");
                            self.cells[self.cursor.1].fill(" ".into());
                        }
                        b'm' => {}
                        other => panic!("unexpected output command {other}"),
                    }
                    text = &rest[end + 1..];
                } else {
                    let grapheme = text.graphemes(true).next().unwrap();
                    text = &text[grapheme.len()..];
                    match grapheme {
                        "\n" => self.down(),
                        "\r" => self.cursor.0 = 0,
                        _ => {
                            let width = grapheme.width();
                            if width == 0 {
                                continue;
                            }
                            if self.cursor.0 + width > self.cells[0].len() {
                                self.cursor.0 = 0;
                                self.down();
                            }
                            self.cells[self.cursor.1][self.cursor.0] = grapheme.into();
                            for offset in 1..width {
                                self.cells[self.cursor.1][self.cursor.0 + offset].clear();
                            }
                            self.cursor.0 += width;
                            if self.cursor.0 == self.cells[0].len() {
                                self.cursor.0 = 0;
                                self.down();
                            }
                        }
                    }
                }
            }
        }
        fn rows(&self) -> Vec<String> {
            self.cells
                .iter()
                .map(|row| row.concat().trim_end().to_owned())
                .collect()
        }
    }

    #[test]
    fn old_logical_output_scrolls_but_physical_rows_remain_in_viewport() {
        let mut state = TuiState::new(PathBuf::from("long/workspace/path/".repeat(12)), true);
        state.status = "long error ".repeat(35);
        state.apply_input(InputEvent::Text("abc".into()));
        let mut old = Screen::new(24, 8);
        old.feed(state.render(24).as_bytes());
        assert!(
            old.scrolls > 0,
            "the previous layout overflow must be reproduced"
        );
        let mut renderer = TtyRenderer::new(Vec::new());
        let mut screen = Screen::new(24, 8);
        renderer.render_at(&state, 24, 8).unwrap();
        screen.feed(&renderer.writer);
        assert_eq!(screen.scrolls, 0);
        assert_eq!(
            screen
                .rows()
                .iter()
                .filter(|row| row.contains("\u{203a} abc"))
                .count(),
            1
        );
        assert_eq!(screen.cursor, (5, 6));
        assert_eq!(
            screen.rows(),
            renderer
                .previous
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.trim_end().to_owned())
                .collect::<Vec<_>>()
        );
        state.status = "Ready".into();
        let before = renderer.writer.len();
        renderer.render_at(&state, 24, 8).unwrap();
        screen.feed(&renderer.writer[before..]);
        assert_eq!(
            screen.rows(),
            renderer
                .previous
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.trim_end().to_owned())
                .collect::<Vec<_>>()
        );
        assert!(!screen.rows().join("\n").contains("long error"));
        assert_eq!(renderer.stats.full_clears, 1);
    }

    #[test]
    fn unicode_tabs_multiline_resize_and_cursor_only_frames_reconstruct() {
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        state.apply_input(InputEvent::Text(
            "bookkeeper\nПривет é e\u{301} 界\tend \\r\\n".into(),
        ));
        let mut renderer = TtyRenderer::new(Vec::new());
        for (width, height) in [(22, 8), (80, 24), (12, 5), (2, 2), (1, 1)] {
            let before = renderer.writer.len();
            renderer.render_at(&state, width, height).unwrap();
            let mut screen = Screen::new(width as usize, height as usize);
            screen.feed(&renderer.writer[before..]);
            let frame = renderer.previous.as_ref().unwrap();
            assert_eq!(
                screen.rows(),
                frame
                    .rows
                    .iter()
                    .map(|r| r.trim_end().to_owned())
                    .collect::<Vec<_>>()
            );
            assert!(frame
                .rows
                .iter()
                .all(|row| row.width() <= usize::from(width.saturating_sub(1).max(1))));
            if width > 1 {
                assert_eq!(screen.scrolls, 0);
            }
        }
        state.clear_composer();
        state.apply_input(InputEvent::Text("abc".into()));
        renderer.render_at(&state, 80, 24).unwrap();
        let before = renderer.writer.len();
        let rows_before = renderer.stats.changed_rows;
        state.apply_input(InputEvent::Left);
        renderer.render_at(&state, 80, 24).unwrap();
        assert_eq!(&renderer.writer[before..], b"\x1b[22;7H");
        assert_eq!(renderer.stats.changed_rows, rows_before);
        let after = renderer.writer.len();
        let flushes = renderer.stats.flushes;
        for _ in 0..100 {
            renderer.render_at(&state, 80, 24).unwrap();
        }
        assert_eq!(renderer.writer.len(), after);
        assert_eq!(renderer.stats.flushes, flushes);
    }

    #[test]
    fn display_layout_preserves_combining_and_wide_graphemes_and_neutralizes_controls() {
        assert_eq!(wrap("é e\u{301} 界", 4), ["é e\u{301} ", "界"]);
        assert_eq!(wrap("a\tb", 8), ["a   b"]);
        assert_eq!(wrap("a\x1b[2J", 8), ["a?[2J"]);
        assert_eq!(wrap("first\nsecond", 8), ["first", "second"]);
    }

    // Writes real renderer output for visual review. Set ARGENTUM_TUI_PREVIEW_DIR and run
    // this ignored test, then print a file in a terminal.
    #[test]
    #[ignore = "writes files for manual visual review"]
    fn write_visual_preview() {
        use crate::tui::{TimelineEntry, TimelineKind};
        use argentum_domain::RunId;
        let directory = std::env::var("ARGENTUM_TUI_PREVIEW_DIR").expect("preview directory");
        let entry = |kind, text: &str, detail: &str, state: &str| TimelineEntry {
            view_id: RunId::new_v4(),
            message_id: None,
            kind,
            run_id: None,
            tool_call_id: None,
            text: text.into(),
            detail: detail.into(),
            state: state.into(),
        };
        let base = || {
            let mut state = TuiState::new(PathBuf::from("C:\\projects\\session-store"), false);
            state.model = "qwen3-coder-next".into();
            state.provider_label = "LM Studio".into();
            state.permission_mode = "Guided".into();
            state
        };
        let conversation = || {
            let mut state = base();
            state.active_session_id = Some(argentum_domain::SessionId::new_v4());
            state.context_use = Some((state.active_session_id.unwrap(), 41_000, 180_000));
            state.timeline = vec![
                entry(TimelineKind::User, "Imports of large session files take over a minute. Can you find out why and fix it?", "", ""),
                entry(TimelineKind::Tool, "search_text: \"insert_session\" in src", "duration: 18 ms", "succeeded"),
                entry(TimelineKind::Tool, "read_text: src/import.rs", "duration: 4 ms", "succeeded"),
                entry(TimelineKind::Tool, "apply_patch: Modified \"src/import.rs\"\nCreated \"tests/import.rs\"", "duration: 22 ms", "succeeded"),
                entry(TimelineKind::Tool, "cargo-test-locked: 214 tests", "duration: 41200 ms", "failed"),
                entry(
                    TimelineKind::Assistant,
                    "I found the problem. The store opens a new transaction per row, so a large import holds the write lock for **minutes**.\n\n## What I changed\n\n- Batched the inserts in `import_sessions`.\n- Added a `busy_timeout` so readers wait instead of failing.\n\n```rust\nlet tx = connection.transaction()?;\nfor row in rows {\n    insert_session(&tx, &row)?;\n}\ntx.commit()?;\n```\n\n> The import went from 94 s to 2.1 s on the sample file.\n\nSee the [SQLite docs](https://sqlite.org/lang_transaction.html) for why this matters.",
                    "",
                    "",
                ),
            ];
            state
        };
        let mut scenes = vec![("welcome", base()), ("conversation", conversation())];
        let mut running = conversation();
        running.active_run_id = Some(RunId::new_v4());
        running.status = "Waiting for model".into();
        running.progress_tick = 2;
        scenes.push(("running", running));
        let mut approval = conversation();
        approval.pending_approval = Some(argentum_domain::ApprovalRequest {
            id: RunId::new_v4(),
            run_id: RunId::new_v4(),
            tool_id: "write_text".into(),
            action: "Write file".into(),
            target: "src/import.rs".into(),
            binding: argentum_domain::Sha256Digest("0".repeat(64)),
            reason: "Apply the batched insert change.".into(),
            capabilities: vec![argentum_domain::Capability::WriteFiles],
            scope: argentum_domain::ApprovalScope::Once,
            expires_at: None,
        });
        scenes.push(("approval", approval));
        let mut palette = conversation();
        palette.apply_input(InputEvent::Text("/".into()));
        scenes.push(("palette", palette));
        let mut typing = base();
        typing.apply_input(InputEvent::Text("Add a retry to the provider probe".into()));
        scenes.push(("typing", typing));
        for (name, state) in scenes {
            let mut renderer = TtyRenderer::new(Vec::new());
            renderer.render_at(&state, 100, 30).unwrap();
            std::fs::write(format!("{directory}/{name}.ans"), &renderer.writer).unwrap();
        }
    }
}
