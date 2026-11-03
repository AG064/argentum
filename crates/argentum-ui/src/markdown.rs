//! Turns Markdown into the block model the shell renders. Text comes from models
//! and web pages, so it is treated as untrusted: raw HTML stays literal and links
//! keep only http and https targets.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use slint::{ModelRc, VecModel};

use crate::RichBlockView;

const MAX_BLOCKS: usize = 1500;

fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if matches!(
            character,
            '\\' | '`'
                | '*'
                | '_'
                | '['
                | ']'
                | '<'
                | '>'
                | '&'
                | '~'
                | '!'
                | '('
                | ')'
                | '|'
                | '#'
        ) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

fn safe_link(target: &str) -> Option<String> {
    let target = target.trim();
    let lower = target.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    Some(
        target
            .replace('(', "%28")
            .replace(')', "%29")
            .replace(' ', "%20"),
    )
}

pub(crate) fn make_block(
    kind: &str,
    text: String,
    markdown: Option<&str>,
    lang: &str,
    depth: i32,
    marker: &str,
) -> RichBlockView {
    let styled = markdown.and_then(|markdown| slint::StyledText::from_markdown(markdown).ok());
    RichBlockView {
        kind: kind.into(),
        rich: styled.is_some(),
        styled: styled.unwrap_or_default(),
        text: text.into(),
        lang: lang.into(),
        depth,
        marker: marker.into(),
    }
}

#[derive(Default)]
struct Inline {
    plain: String,
    markdown: String,
    formatted: bool,
}

impl Inline {
    fn text(&mut self, text: &str) {
        self.plain.push_str(text);
        self.markdown.push_str(&escape_inline(text));
    }

    fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

struct ListState {
    next: Option<u64>,
}

pub fn parse(source: &str) -> Vec<RichBlockView> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let mut blocks = Vec::new();
    let mut inline = Inline::default();
    let mut lists: Vec<ListState> = Vec::new();
    let mut marker = String::new();
    let mut quote_depth = 0usize;
    let mut code: Option<(String, String)> = None;
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut in_table = false;
    let mut link_open = false;

    let flush = |inline: &mut Inline,
                 blocks: &mut Vec<RichBlockView>,
                 kind: &str,
                 depth: i32,
                 marker: &str| {
        let piece = inline.take();
        if piece.plain.trim().is_empty() {
            return;
        }
        let text = piece.plain.trim_end().to_owned();
        let markdown = piece.formatted.then_some(piece.markdown.trim().to_owned());
        blocks.push(make_block(
            kind,
            text,
            markdown.as_deref(),
            "",
            depth,
            marker,
        ));
    };

    for event in Parser::new_ext(source, options) {
        if blocks.len() >= MAX_BLOCKS {
            break;
        }
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang, body)) = code.take() {
                    blocks.push(make_block(
                        "code",
                        body.trim_end_matches('\n').to_owned(),
                        None,
                        &lang,
                        0,
                        "",
                    ));
                }
            }
            Event::Text(text) if code.is_some() => {
                if let Some((_, body)) = code.as_mut() {
                    body.push_str(&text);
                }
            }
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                table.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                let rendered = render_table(&table);
                blocks.push(make_block("table", rendered, None, "", 0, ""));
            }
            Event::Start(Tag::TableHead | Tag::TableRow) if in_table => table.push(Vec::new()),
            Event::Start(Tag::TableCell) if in_table => inline = Inline::default(),
            Event::End(TagEnd::TableCell) if in_table => {
                let cell = inline.take().plain.trim().to_owned();
                if let Some(row) = table.last_mut() {
                    row.push(cell);
                }
            }
            Event::Start(Tag::List(first)) => {
                let depth = lists.len().saturating_sub(1) as i32;
                flush(&mut inline, &mut blocks, "li", depth, &marker);
                lists.push(ListState { next: first });
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let depth = lists.len().saturating_sub(1) as i32;
                flush(&mut inline, &mut blocks, "li", depth, &marker);
                marker = match lists.last_mut() {
                    Some(ListState { next: Some(next) }) => {
                        let label = format!("{next}.");
                        *next += 1;
                        label
                    }
                    _ => "\u{2022}".to_owned(),
                };
            }
            Event::End(TagEnd::Item) => {
                let depth = lists.len().saturating_sub(1) as i32;
                flush(&mut inline, &mut blocks, "li", depth, &marker);
            }
            Event::Start(Tag::Paragraph) | Event::Start(Tag::Heading { .. }) => {}
            Event::End(TagEnd::Paragraph) => {
                if lists.is_empty() {
                    let kind = if quote_depth > 0 { "quote" } else { "p" };
                    flush(&mut inline, &mut blocks, kind, 0, "");
                } else {
                    // Tight and loose list items both collect into one block, with
                    // a following nested list flushing it.
                    inline.plain.push(' ');
                    inline.markdown.push(' ');
                }
            }
            Event::End(TagEnd::Heading(level)) => {
                let kind = match level {
                    HeadingLevel::H1 => "h1",
                    HeadingLevel::H2 => "h2",
                    _ => "h3",
                };
                flush(&mut inline, &mut blocks, kind, 0, "");
            }
            Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
            Event::Rule => blocks.push(make_block("hr", String::new(), None, "", 0, "")),
            Event::Start(Tag::Strong) | Event::End(TagEnd::Strong) => {
                inline.markdown.push_str("**");
                inline.formatted = true;
            }
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => {
                inline.markdown.push('*');
                inline.formatted = true;
            }
            Event::Start(Tag::Strikethrough) | Event::End(TagEnd::Strikethrough) => {
                inline.markdown.push_str("~~");
                inline.formatted = true;
            }
            Event::Code(text) => {
                inline.plain.push_str(&text);
                let ticks = if text.contains('`') { "``" } else { "`" };
                inline.markdown.push_str(ticks);
                inline.markdown.push_str(&text);
                inline.markdown.push_str(ticks);
                inline.formatted = true;
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_open = false;
                if let Some(target) = safe_link(&dest_url) {
                    inline.markdown.push('[');
                    inline.formatted = true;
                    link_open = true;
                    // Pushed on close, so remember it in the markdown buffer's tail.
                    LINK_TARGET.with(|cell| cell.replace(target));
                }
            }
            Event::End(TagEnd::Link) => {
                if link_open {
                    let target = LINK_TARGET.with(|cell| cell.take());
                    inline.markdown.push_str("](");
                    inline.markdown.push_str(&target);
                    inline.markdown.push(')');
                    link_open = false;
                }
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => inline.text(&text),
            Event::SoftBreak => inline.text(" "),
            Event::HardBreak => {
                inline.plain.push('\n');
                inline.markdown.push_str("  \n");
            }
            _ => {}
        }
    }
    flush(&mut inline, &mut blocks, "p", 0, "");
    blocks
}

thread_local! {
    static LINK_TARGET: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn render_table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let mut out = String::new();
    for (index, row) in rows.iter().enumerate() {
        let line = (0..columns)
            .map(|column| {
                let cell = row.get(column).map_or("", String::as_str);
                let pad = widths[column].saturating_sub(cell.chars().count());
                format!("{cell}{}", " ".repeat(pad))
            })
            .collect::<Vec<_>>()
            .join("  ");
        out.push_str(line.trim_end());
        out.push('\n');
        if index == 0 && rows.len() > 1 {
            let rule = widths
                .iter()
                .map(|width| "-".repeat(*width))
                .collect::<Vec<_>>()
                .join("  ");
            out.push_str(&rule);
            out.push('\n');
        }
    }
    out.trim_end().to_owned()
}

pub fn model(source: &str) -> ModelRc<RichBlockView> {
    ModelRc::new(VecModel::from(parse(source)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<String> {
        parse(source)
            .iter()
            .map(|block| block.kind.to_string())
            .collect()
    }

    #[test]
    fn splits_common_structure() {
        let source = "# Title\n\nSome **bold** text.\n\n- one\n- two\n\n```rust\nfn a() {}\n```\n\n> quote\n\n---\n";
        assert_eq!(
            kinds(source),
            ["h1", "p", "li", "li", "code", "quote", "hr"]
        );
    }

    #[test]
    fn plain_paragraphs_stay_selectable_and_formatted_ones_use_rich_text() {
        let blocks = parse("just words\n\nwith `code` inside");
        assert!(!blocks[0].rich);
        assert_eq!(blocks[0].text, "just words");
        assert!(blocks[1].rich);
        assert_eq!(blocks[1].text, "with code inside");
    }

    #[test]
    fn code_blocks_keep_language_and_text() {
        let blocks = parse("```python title=x\nprint(1)\n  print(2)\n```");
        assert_eq!(blocks[0].lang, "python");
        assert_eq!(blocks[0].text, "print(1)\n  print(2)");
    }

    #[test]
    fn ordered_lists_number_from_their_start() {
        let blocks = parse("3. a\n4. b\n");
        assert_eq!(blocks[0].marker, "3.");
        assert_eq!(blocks[1].marker, "4.");
    }

    #[test]
    fn nested_lists_gain_depth() {
        let blocks = parse("- a\n  - b\n- c\n");
        let shape = blocks
            .iter()
            .map(|block| (block.text.to_string(), block.depth))
            .collect::<Vec<_>>();
        assert_eq!(
            shape,
            [
                ("a".to_owned(), 0),
                ("b".to_owned(), 1),
                ("c".to_owned(), 0)
            ]
        );
    }

    #[test]
    fn tables_render_as_aligned_text() {
        let blocks = parse("| A | Long |\n| - | - |\n| 1 | 2 |\n");
        assert_eq!(blocks[0].kind, "table");
        assert_eq!(blocks[0].text, "A  Long\n-  ----\n1  2");
    }

    #[test]
    fn unsafe_links_and_raw_html_stay_inert() {
        let blocks = parse("[x](javascript:alert(1)) <font color=red>y</font>");
        assert!(blocks[0].text.contains('x'));
        assert!(blocks[0].text.contains("<font color=red>"));
        // Nothing here counts as a link, so the block stays plain text.
        assert!(!blocks[0].rich);
    }

    #[test]
    fn incomplete_streaming_markdown_never_panics() {
        for source in ["```rust\nfn", "**bold", "- ", "| a |", "[link](", "> "] {
            let _ = parse(source);
        }
    }
}
