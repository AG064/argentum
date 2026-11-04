use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{TimelineEntry, TimelineKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    Plain,
    Strong,
    Muted,
    Faint,
    Accent,
    Danger,
    Code,
    Selected,
}

impl Tone {
    fn sgr(self, no_color: bool) -> &'static str {
        match (self, no_color) {
            (Tone::Plain, _) => "",
            (Tone::Strong | Tone::Selected | Tone::Danger, true) => "1",
            (_, true) => "",
            (Tone::Strong, false) => "1",
            (Tone::Muted, false) => "38;5;245",
            (Tone::Faint, false) => "38;5;240",
            (Tone::Accent, false) => "38;5;167",
            (Tone::Danger, false) => "1;38;5;167",
            (Tone::Code, false) => "38;5;174",
            (Tone::Selected, false) => "1;38;5;167",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Span {
    pub start: usize,
    pub end: usize,
    pub tone: Tone,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Row {
    pub text: String,
    pub spans: Vec<Span>,
}

impl Row {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            spans: Vec::new(),
        }
    }

    pub fn toned(text: impl Into<String>, tone: Tone) -> Self {
        let mut row = Self::default();
        row.push(&text.into(), tone);
        row
    }

    // Keeps whole graphemes and the spans that still apply, so a narrow window
    // can never make a row wrap and push the composer off the screen.
    pub fn clip(&mut self, columns: usize) {
        if cells(&self.text) <= columns {
            return;
        }
        let mut used = 0;
        let mut keep = 0;
        for (offset, grapheme) in self.text.grapheme_indices(true) {
            used += cells(grapheme);
            if used > columns {
                break;
            }
            keep = offset + grapheme.len();
        }
        self.text.truncate(keep);
        self.spans.retain_mut(|span| {
            span.end = span.end.min(keep);
            span.start < span.end
        });
    }

    pub fn push(&mut self, text: &str, tone: Tone) {
        let start = self.text.len();
        self.text.push_str(text);
        if tone == Tone::Plain || text.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last) if last.end == start && last.tone == tone => last.end = self.text.len(),
            _ => self.spans.push(Span {
                start,
                end: self.text.len(),
                tone,
            }),
        }
    }
}

// Colors come only from Tone. Row text has already been sanitized, so remote
// content can never carry its own escape sequence into the terminal.
pub(super) fn paint(row: &Row, no_color: bool) -> String {
    let mut out = String::with_capacity(row.text.len());
    let mut at = 0;
    for span in &row.spans {
        out.push_str(&row.text[at..span.start]);
        let sgr = span.tone.sgr(no_color);
        if sgr.is_empty() {
            out.push_str(&row.text[span.start..span.end]);
        } else {
            out.push_str("\x1b[");
            out.push_str(sgr);
            out.push('m');
            out.push_str(&row.text[span.start..span.end]);
            out.push_str("\x1b[0m");
        }
        at = span.end;
    }
    out.push_str(&row.text[at..]);
    out
}

pub(super) fn cells(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

#[derive(Debug, Clone)]
struct Line {
    first: &'static str,
    rest: &'static str,
    prefix_tone: Tone,
    segments: Vec<(String, Tone)>,
    word_wrap: bool,
}

impl Line {
    fn new(prefix: &'static str, prefix_tone: Tone, segments: Vec<(String, Tone)>) -> Self {
        Self {
            first: prefix,
            rest: prefix,
            prefix_tone,
            segments,
            word_wrap: true,
        }
    }

    fn hanging(mut self, rest: &'static str) -> Self {
        self.rest = rest;
        self
    }
}

// Inline Markdown changes emphasis only. A link keeps its visible address, so a
// model cannot hide where a link goes behind friendly text.
fn inline(text: &str, base: Tone) -> Vec<(String, Tone)> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(index) = rest.find(['`', '*', '[']) {
        plain.push_str(&rest[..index]);
        let tail = &rest[index..];
        let span = if let Some(body) = tail.strip_prefix('`') {
            body.find('`')
                .filter(|end| *end > 0)
                .map(|end| (body[..end].to_owned(), Tone::Code, 1 + end + 1))
        } else if let Some(body) = tail.strip_prefix("**") {
            body.find("**")
                .filter(|end| *end > 0)
                .map(|end| (body[..end].to_owned(), Tone::Strong, 2 + end + 2))
        } else if let Some(body) = tail.strip_prefix('[') {
            body.find("](").and_then(|middle| {
                let address = &body[middle + 2..];
                address.find(')').filter(|end| *end > 0).map(|end| {
                    (
                        format!("{} ({})", &body[..middle], &address[..end]),
                        Tone::Accent,
                        1 + middle + 2 + end + 1,
                    )
                })
            })
        } else {
            None
        };
        match span {
            Some((body, tone, used)) => {
                if !plain.is_empty() {
                    out.push((std::mem::take(&mut plain), base));
                }
                if tone == Tone::Strong {
                    out.extend(inline(&body, Tone::Strong));
                } else {
                    out.push((body, tone));
                }
                rest = &tail[used..];
            }
            None => {
                let width = tail.chars().next().map_or(1, char::len_utf8);
                plain.push_str(&tail[..width]);
                rest = &tail[width..];
            }
        }
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        out.push((plain, base));
    }
    out
}

fn markdown(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut fenced = false;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        if let Some(language) = trimmed.strip_prefix("```") {
            fenced = !fenced;
            if fenced && !language.trim().is_empty() {
                lines.push(Line::new(
                    "  ",
                    Tone::Plain,
                    vec![(language.trim().to_owned(), Tone::Faint)],
                ));
            }
            continue;
        }
        if fenced {
            let mut code = Line::new(
                "  \u{2502} ",
                Tone::Faint,
                vec![(line.to_owned(), Tone::Plain)],
            );
            code.word_wrap = false;
            lines.push(code);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            lines.push(Line::new(
                "  ",
                Tone::Plain,
                inline(trimmed[hashes..].trim(), Tone::Strong),
            ));
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            lines.push(
                Line::new("  \u{2022} ", Tone::Muted, inline(item, Tone::Plain)).hanging("    "),
            );
        } else if let Some(quote) = trimmed.strip_prefix("> ") {
            lines.push(Line::new(
                "  \u{2502} ",
                Tone::Faint,
                inline(quote, Tone::Muted),
            ));
        } else {
            lines.push(Line::new("  ", Tone::Plain, inline(line, Tone::Plain)));
        }
    }
    lines
}

fn blank() -> Line {
    Line::new("", Tone::Plain, Vec::new())
}

fn plain_lines(text: &str, prefix: &'static str, tone: Tone) -> Vec<Line> {
    text.split('\n')
        .map(|line| Line::new(prefix, Tone::Plain, vec![(line.to_owned(), tone)]))
        .collect()
}

fn lines_for(entry: &TimelineEntry) -> Vec<Line> {
    let mut lines = Vec::new();
    match entry.kind {
        TimelineKind::User => {
            lines.push(blank());
            let mut body = plain_lines(&entry.text, "  ", Tone::Strong);
            if let Some(first) = body.first_mut() {
                first.first = "\u{203a} ";
                first.prefix_tone = Tone::Accent;
            }
            lines.append(&mut body);
        }
        TimelineKind::Assistant => {
            lines.push(blank());
            if !entry.detail.is_empty() {
                lines.push(Line::new(
                    "  ",
                    Tone::Plain,
                    vec![("Thinking".to_owned(), Tone::Muted)],
                ));
                // The tail is enough to follow along. A long trace must not push the
                // answer off the screen.
                let tail = entry.detail.trim_end().rsplitn(7, '\n').collect::<Vec<_>>();
                let shown = tail[..tail.len().min(6)]
                    .iter()
                    .rev()
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n");
                lines.append(&mut plain_lines(&shown, "  \u{2502} ", Tone::Faint));
            }
            let mut body = markdown(&entry.text);
            if let Some(first) = body.first_mut() {
                if first.first == "  " {
                    first.first = "\u{25cf} ";
                    first.prefix_tone = Tone::Accent;
                }
            }
            lines.append(&mut body);
            if matches!(entry.state.as_str(), "failed" | "cancelled" | "interrupted") {
                lines.push(Line::new(
                    "  ",
                    Tone::Plain,
                    vec![(format!("Response {}", entry.state), Tone::Danger)],
                ));
            }
        }
        TimelineKind::Tool => {
            let (tool, summary) = entry
                .text
                .split_once(": ")
                .unwrap_or((entry.text.as_str(), ""));
            let mut summary_lines = summary.split('\n').map(|line| line.trim_end_matches('\r'));
            let first_summary = summary_lines.next().unwrap_or("");
            let failed = matches!(
                entry.state.as_str(),
                "failed" | "cancelled" | "not executed"
            );
            let running = entry.state == "running";
            let mut segments = vec![(tool.to_owned(), Tone::Strong)];
            if !first_summary.is_empty() {
                segments.push((format!("  {first_summary}"), Tone::Muted));
            }
            // Only worth showing once it is long enough to notice.
            if let Some(seconds) = entry
                .detail
                .strip_prefix("duration: ")
                .and_then(|duration| duration.strip_suffix(" ms"))
                .and_then(|millis| millis.parse::<u64>().ok())
                .filter(|millis| *millis >= 1000)
                .map(|millis| millis as f64 / 1000.0)
            {
                segments.push((format!("  {seconds:.1}s"), Tone::Faint));
            }
            if running {
                segments.push(("  running".into(), Tone::Faint));
            }
            lines.push(
                Line::new(
                    "\u{25cf} ",
                    if failed {
                        Tone::Danger
                    } else if running {
                        Tone::Faint
                    } else {
                        Tone::Muted
                    },
                    segments,
                )
                .hanging("  "),
            );
            for line in summary_lines {
                lines.push(Line::new(
                    "  \u{2514} ",
                    Tone::Faint,
                    vec![(line.to_owned(), Tone::Muted)],
                ));
            }
            if failed {
                lines.push(Line::new(
                    "  \u{2514} ",
                    Tone::Faint,
                    vec![(entry.state.clone(), Tone::Danger)],
                ));
            }
        }
        TimelineKind::Approval => {
            lines.push(blank());
            lines.push(
                Line::new(
                    "! ",
                    Tone::Danger,
                    vec![(entry.text.replace(['\n', '\r'], " "), Tone::Danger)],
                )
                .hanging("  "),
            );
            lines.append(&mut plain_lines(&entry.detail, "  ", Tone::Plain));
            if !entry.state.is_empty() {
                lines.push(Line::new(
                    "  ",
                    Tone::Plain,
                    vec![(entry.state.clone(), Tone::Muted)],
                ));
            }
        }
        TimelineKind::System => {
            lines.push(blank());
            let tone = if entry.text.contains(" failed") {
                Tone::Danger
            } else {
                Tone::Muted
            };
            lines.append(&mut plain_lines(&entry.text, "  ", tone));
            if !entry.detail.is_empty() {
                lines.append(&mut plain_lines(&entry.detail, "  ", Tone::Muted));
            }
        }
    }
    lines
}

fn sanitize(grapheme: &str) -> String {
    if grapheme == "\t" {
        "    ".into()
    } else if grapheme.chars().any(char::is_control) {
        "?".into()
    } else {
        grapheme.to_owned()
    }
}

// Offsets rise strictly within an entry and do not depend on the width, so a
// reading position survives a resize. Word wrap never loses text: only the
// breaking space itself is dropped from the end of a row.
fn wrap_line(line: &Line, width: usize, base: usize) -> Vec<(usize, Row)> {
    let mut units = Vec::new();
    let mut offset = 0;
    for (text, tone) in &line.segments {
        for grapheme in text.graphemes(true) {
            units.push((sanitize(grapheme), *tone, offset));
            offset += grapheme.len();
        }
    }
    let prefix = cells(line.first).max(cells(line.rest));
    let available = width.saturating_sub(prefix).max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    loop {
        let mut used = 0;
        let mut end = start;
        let mut last_space = None;
        while end < units.len() {
            let unit_cells = cells(&units[end].0).min(available);
            if used + unit_cells > available {
                break;
            }
            if units[end].0 == " " {
                last_space = Some(end);
            }
            used += unit_cells;
            end += 1;
        }
        let at_space = end < units.len() && units[end].0 == " " && line.word_wrap;
        let (stop, next) = match last_space {
            _ if at_space => (end, end + 1),
            Some(space) if end < units.len() && line.word_wrap && space > start => {
                (space, space + 1)
            }
            _ => (
                end.max(start + usize::from(start < units.len())),
                end.max(start + 1),
            ),
        };
        let mut row = Row::default();
        row.push(
            if rows.is_empty() {
                line.first
            } else {
                line.rest
            },
            line.prefix_tone,
        );
        for (text, tone, _) in &units[start..stop.min(units.len())] {
            let display = if cells(text) > available { "?" } else { text };
            row.push(display, *tone);
        }
        let at = base + units.get(start).map_or(0, |unit| unit.2);
        rows.push((at, row));
        if next >= units.len() {
            break;
        }
        start = next;
    }
    rows
}

pub(super) fn entry_rows(entry: &TimelineEntry, width: usize) -> Vec<(usize, Row)> {
    let mut rows = Vec::new();
    let mut base = 0;
    for line in lines_for(entry) {
        let length: usize = line.segments.iter().map(|(text, _)| text.len()).sum();
        rows.extend(wrap_line(&line, width, base));
        base += length + 1;
    }
    rows
}

// The overlays stay plain text builders. This only adds emphasis, so their
// wording and their reviewed safety text are unchanged.
pub(super) fn overlay_row(index: usize, text: String) -> Row {
    if index == 0 {
        Row::toned(text, Tone::Strong)
    } else if text.starts_with("> ") {
        Row::toned(text, Tone::Selected)
    } else {
        Row::plain(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(rows: &[(usize, Row)]) -> Vec<String> {
        rows.iter().map(|(_, row)| row.text.clone()).collect()
    }

    #[test]
    fn inline_markdown_removes_markers_and_keeps_link_addresses_visible() {
        let segments = inline(
            "**bold** and `code` see [docs](https://example.test/a)",
            Tone::Plain,
        );
        let joined: String = segments.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(joined, "bold and code see docs (https://example.test/a)");
        assert_eq!(segments[0], ("bold".into(), Tone::Strong));
        assert!(segments
            .iter()
            .any(|(text, tone)| text == "code" && *tone == Tone::Code));
    }

    #[test]
    fn code_inside_bold_loses_its_markers() {
        let joined: String = inline("**`notes.txt` reads:**", Tone::Plain)
            .iter()
            .map(|(text, _)| text.as_str())
            .collect();
        assert_eq!(joined, "notes.txt reads:");
    }

    #[test]
    fn unmatched_markers_stay_literal_while_a_response_streams() {
        let joined: String = inline("a **partial and `open [x", Tone::Plain)
            .iter()
            .map(|(text, _)| text.clone())
            .collect();
        assert_eq!(joined, "a **partial and `open [x");
    }

    #[test]
    fn word_wrap_breaks_on_spaces_and_hard_wraps_long_words() {
        let line = Line::new(
            "  ",
            Tone::Plain,
            vec![("one two three".into(), Tone::Plain)],
        );
        assert_eq!(text(&wrap_line(&line, 9, 0)), ["  one two", "  three"]);
        let line = Line::new("  ", Tone::Plain, vec![("abcdefghij".into(), Tone::Plain)]);
        assert_eq!(text(&wrap_line(&line, 6, 0)), ["  abcd", "  efgh", "  ij"]);
    }

    #[test]
    fn offsets_rise_strictly_so_scrolling_cannot_stick() {
        let line = Line::new(
            "  ",
            Tone::Plain,
            vec![("one two three four".into(), Tone::Plain)],
        );
        let offsets: Vec<usize> = wrap_line(&line, 8, 5).iter().map(|(at, _)| *at).collect();
        assert_eq!(offsets[0], 5);
        assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn model_content_cannot_emit_escape_sequences_or_reach_column_zero() {
        let line = Line::new("  ", Tone::Plain, inline("\x1b[31mred\x1b[0m", Tone::Plain));
        let rows = wrap_line(&line, 40, 0);
        let painted = paint(&rows[0].1, false);
        assert!(!painted.contains('\x1b'));
        assert!(rows[0].1.text.starts_with("  "));
        for parsed in markdown("x\n# heading\n- item\n> quote\n```\ncode\n```") {
            assert!(parsed.first.starts_with(' '));
        }
    }

    #[test]
    fn clipping_respects_wide_graphemes_and_trims_spans() {
        let mut row = Row::toned("ab\u{754c}\u{754c}", Tone::Muted);
        row.clip(3);
        assert_eq!(row.text, "ab");
        assert_eq!(row.spans[0].end, 2);
        row.clip(0);
        assert!(row.text.is_empty() && row.spans.is_empty());
    }

    #[test]
    fn no_color_keeps_emphasis_and_drops_color() {
        let row = Row::toned("x", Tone::Muted);
        assert_eq!(paint(&row, true), "x");
        assert!(paint(&row, false).contains("38;5;245"));
        assert_eq!(
            paint(&Row::toned("x", Tone::Selected), true),
            "\x1b[1mx\x1b[0m"
        );
    }
}
