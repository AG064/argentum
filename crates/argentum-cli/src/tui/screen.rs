use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::TuiState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub rows: Vec<String>,
    pub cursor: (u16, u16),
}

// Keep one column unused: writing the last cell can trigger wrapping or scroll
// immediately on Windows. No emitted row depends on the host's newline mode.
pub(super) fn frame(state: &TuiState, width: u16, height: u16) -> Frame {
    let columns = usize::from(width.saturating_sub(1).clamp(1, 512));
    let height = usize::from(height.clamp(1, 200));
    if width <= 1 {
        return Frame {
            rows: vec![String::new(); height],
            cursor: (0, (height - 1) as u16),
        };
    }
    let text = format!("> {}", state.composer);
    let (composer, cursor) = layout(&text, columns, Some(state.composer_cursor + 2));
    let composer_space = height.saturating_sub(2).max(1).min(composer.len());
    let start = cursor.1.saturating_add(1).saturating_sub(composer_space);
    let composer_end = (start + composer_space).min(composer.len());
    let hint_rows = usize::from(height > 1);
    let body_space = height.saturating_sub(composer_end - start + hint_rows);
    let mut rows = Vec::new();
    for header in [
        format!(
            "ARGENTUM | {} | access: {}",
            state.provider_label, state.permission_mode
        ),
        format!("status: {}", state.display_status()),
    ] {
        if rows.len() < body_space {
            rows.push(wrap(&header, columns).remove(0));
        }
    }
    if !state.viewport.notice.is_empty() && rows.len() < body_space {
        rows.push(wrap(&state.viewport.notice, columns).remove(0));
    }
    if let Some(skill) = &state.next_skill {
        if rows.len() < body_space {
            rows.push(wrap(&format!("Next message skill: {}", skill.name), columns).remove(0));
        }
    }
    let available = body_space.saturating_sub(rows.len());
    let overlay_start = rows.len();
    let settings_cursor = super::settings::cursor(state, columns, available)
        .map(|(x, y)| (x, y.saturating_add(overlay_start as u16)));
    rows.extend(
        state
            .overlay_rows(columns, available)
            .unwrap_or_else(|| state.transcript_rows(columns, available)),
    );
    rows.resize(body_space, String::new());
    let cursor_row = rows.len() + cursor.1 - start;
    rows.extend_from_slice(&composer[start..composer_end]);
    if hint_rows > 0 {
        let hint = if state.diagnostic_active {
            "CAPTURE ON: F10 stops. Synthetic only."
        } else if state.interrupt_armed {
            "Press Ctrl+C again to exit."
        } else if state.quit_armed {
            "Repeat /quit to discard, /queue continue to keep working."
        } else {
            "F1 help  / commands  PgUp/PgDn history  Ctrl+End latest  F2 access"
        };
        let hint = if let Some(overlay) = &state.overlay {
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
                "Reading | {} new updates | Ctrl+End latest | F1 help | F2 access",
                state.viewport.updates
            )
        } else {
            hint.into()
        };
        rows.push(wrap(&hint, columns).remove(0));
    }
    rows.resize(height, String::new());
    Frame {
        rows,
        cursor: settings_cursor.unwrap_or((
            cursor.0.min(usize::from(width.saturating_sub(1))) as u16,
            cursor_row.min(height - 1) as u16,
        )),
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

// Byte offsets refer to the original message display text, before control
// sanitization. They remain stable when a message is rewrapped on resize.
pub(super) fn wrap_positions(text: &str, width: usize) -> Vec<(usize, String)> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut column = 0;
    let mut start = 0;
    for (offset, g) in text.grapheme_indices(true) {
        if matches!(g, "\n" | "\r\n") {
            rows.push((start, std::mem::take(&mut current)));
            column = 0;
            start = offset + g.len();
            continue;
        }
        let mut display = if g == "\t" {
            " ".repeat((4 - column % 4).min(width))
        } else if g.chars().any(char::is_control) {
            "?".into()
        } else {
            g.to_owned()
        };
        let mut cells = display.width();
        if cells > width {
            display = "?".into();
            cells = 1;
        }
        if column + cells > width {
            rows.push((start, std::mem::take(&mut current)));
            column = 0;
            start = offset;
        }
        current.push_str(&display);
        column += cells;
    }
    rows.push((start, current));
    rows
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
                .filter(|row| row.as_str() == "> abc")
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
        assert_eq!(&renderer.writer[before..], b"\x1b[23;5H");
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
}
