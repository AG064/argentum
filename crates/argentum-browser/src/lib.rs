//! Reader-mode page loading for the Browser module.
//!
//! The shipped application has no JavaScript runtime or dynamic web content
//! execution path, and this crate keeps that true. A page is fetched once over
//! http or https with no cookies, reduced to text blocks, and handed to the shell
//! as data. Page content is untrusted and never reaches a model from here.

use std::time::Duration;

use argentum_domain::{BrowserBlock, BrowserBlockKind, BrowserPage, BrowserPageState};
use url::Url;

const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_BLOCKS: usize = 2000;
const MAX_BLOCK_BYTES: usize = 16 * 1024;
const MAX_TITLE_CHARS: usize = 200;
const MAX_URL_BYTES: usize = 2048;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

fn failed(url: &str, message: impl Into<String>) -> BrowserPage {
    BrowserPage {
        state: BrowserPageState::Failed,
        message: message.into(),
        url: url.to_owned(),
        title: String::new(),
        blocks: Vec::new(),
        truncated: false,
    }
}

/// Turn what someone typed in the address bar into a URL. A bare host gets
/// https. Anything other than http or https is refused, as are embedded
/// credentials.
pub fn normalize_address(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_URL_BYTES {
        return Err("Enter a web address.".to_owned());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("That address contains characters a URL cannot have.".to_owned());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.0.0.1") {
        format!("http://{trimmed}")
    } else if trimmed.contains(':') && !trimmed.contains('.') {
        // A bare scheme such as file: or javascript: is not a host name.
        return Err("Only http and https pages can be opened here.".to_owned());
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&candidate).map_err(|_| "That is not a valid web address.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only http and https pages can be opened here.".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Addresses with embedded credentials are not opened.".to_owned());
    }
    if url.host_str().is_none() {
        return Err("That is not a valid web address.".to_owned());
    }
    Ok(url)
}

pub async fn fetch(address: &str) -> BrowserPage {
    let url = match normalize_address(address) {
        Ok(url) => url,
        Err(message) => return failed(address, message),
    };
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("Argentum-Reader/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(_) => return failed(url.as_str(), "The page could not be requested."),
    };
    let mut response = match client.get(url.clone()).send().await {
        Ok(response) => response,
        Err(error) if error.is_timeout() => {
            return failed(url.as_str(), "The page took too long to respond.")
        }
        Err(_) => return failed(url.as_str(), "The page could not be reached."),
    };
    let final_url = response.url().clone();
    if !matches!(final_url.scheme(), "http" | "https") {
        return failed(
            url.as_str(),
            "The page redirected somewhere that cannot be opened.",
        );
    }
    if !response.status().is_success() {
        return failed(
            final_url.as_str(),
            format!(
                "The server answered with status {}.",
                response.status().as_u16()
            ),
        );
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("text/html")
        .to_ascii_lowercase();
    let is_html = content_type.contains("html");
    if !is_html && !content_type.starts_with("text/") {
        return failed(
            final_url.as_str(),
            "Reader mode only opens web pages and plain text.",
        );
    }
    let mut body = Vec::new();
    let mut truncated = false;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let room = MAX_PAGE_BYTES.saturating_sub(body.len());
                if chunk.len() > room {
                    body.extend_from_slice(&chunk[..room]);
                    truncated = true;
                    break;
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => return failed(final_url.as_str(), "The page stopped loading."),
        }
    }
    let text = String::from_utf8_lossy(&body);
    let (title, blocks, parse_truncated) = if is_html {
        parse_html(&text, &final_url)
    } else {
        parse_plain(&text)
    };
    BrowserPage {
        state: BrowserPageState::Ready,
        message: String::new(),
        url: final_url.to_string(),
        title,
        blocks,
        truncated: truncated || parse_truncated,
    }
}

fn parse_plain(text: &str) -> (String, Vec<BrowserBlock>, bool) {
    let block: String = text.chars().take(MAX_BLOCK_BYTES).collect();
    let truncated = block.len() < text.len();
    let block = block.trim_end().to_owned();
    (
        String::new(),
        vec![BrowserBlock {
            kind: BrowserBlockKind::Code,
            markdown: block.clone(),
            text: block,
        }],
        truncated,
    )
}

/// Escape text so the Markdown renderer treats it literally.
fn escape_markdown(text: &str) -> String {
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
                | '#'
                | '&'
                | '~'
                | '!'
                | '('
                | ')'
                | '|'
        ) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .char_indices()
            .take(12)
            .find(|(_, character)| *character == ';')
            .map(|(index, _)| index);
        let Some(end) = end else {
            out.push('&');
            rest = after;
            continue;
        };
        let name = &after[..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "ndash" | "mdash" => Some('-'),
            "rsquo" | "lsquo" => Some('\''),
            "ldquo" | "rdquo" => Some('"'),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    name.strip_prefix('#')
                        .and_then(|digits| digits.parse().ok())
                })
                .and_then(char::from_u32)
                .filter(|character| !character.is_control() || *character == '\n'),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

struct Tag<'a> {
    name: String,
    closing: bool,
    attributes: &'a str,
    end: usize,
}

/// Read the tag at `start`, which points at `<`. Quoted attribute values may
/// contain `>`.
fn read_tag(html: &str, start: usize) -> Option<Tag<'_>> {
    let bytes = html.as_bytes();
    let mut index = start + 1;
    let closing = bytes.get(index) == Some(&b'/');
    if closing {
        index += 1;
    }
    let name_start = index;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    {
        index += 1;
    }
    if index == name_start {
        return None;
    }
    let name = html[name_start..index].to_ascii_lowercase();
    let attributes_start = index;
    let mut quote = None;
    while let Some(byte) = bytes.get(index) {
        match (quote, *byte) {
            (None, b'>') => {
                return Some(Tag {
                    name,
                    closing,
                    attributes: &html[attributes_start..index],
                    end: index + 1,
                })
            }
            (None, b'"' | b'\'') => quote = Some(*byte),
            (Some(open), byte) if open == byte => quote = None,
            _ => {}
        }
        index += 1;
    }
    None
}

fn attribute<'a>(attributes: &'a str, wanted: &str) -> Option<&'a str> {
    let lower = attributes.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(wanted) {
        let at = search + found;
        let before_ok = at == 0
            || (!lower.as_bytes()[at - 1].is_ascii_alphanumeric()
                && lower.as_bytes()[at - 1] != b'-');
        let after = lower[at + wanted.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let offset = attributes.len() - after.len() + 1;
            let value = attributes[offset..].trim_start();
            return match value.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let inner = &value[1..];
                    inner.find(quote).map(|end| &inner[..end])
                }
                Some(_) => value.split_whitespace().next(),
                None => None,
            };
        }
        search = at + wanted.len();
    }
    None
}

fn collapsed_title(raw: &str) -> String {
    decode_entities(raw)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect()
}

/// Elements whose whole content is dropped.
const SKIPPED: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "iframe", "object", "embed", "canvas",
    "video", "audio", "map", "nav", "aside", "footer", "form", "button", "select", "textarea",
    "dialog",
];

/// Marks a link whose label is still being collected. NUL cannot come from page
/// text because Rust's lossy decode keeps it, so it is stripped from input first.
const LINK_MARK: char = '\u{0}';

struct Builder {
    blocks: Vec<BrowserBlock>,
    kind: BrowserBlockKind,
    markdown: String,
    plain: String,
    truncated: bool,
}

impl Builder {
    fn flush(&mut self) {
        let plain = self.plain.trim_end();
        if !plain.trim_start().is_empty() {
            if self.blocks.len() >= MAX_BLOCKS {
                self.truncated = true;
            } else {
                let kind = self.kind;
                let (markdown, text) = if kind == BrowserBlockKind::Code {
                    (plain.to_owned(), plain.to_owned())
                } else {
                    (
                        self.markdown.replace(LINK_MARK, "").trim().to_owned(),
                        plain.trim_start().to_owned(),
                    )
                };
                self.blocks.push(BrowserBlock {
                    kind,
                    markdown,
                    text,
                });
            }
        }
        self.markdown.clear();
        self.plain.clear();
    }

    fn start(&mut self, kind: BrowserBlockKind) {
        self.flush();
        self.kind = kind;
    }

    fn push_text(&mut self, text: &str, preformatted: bool) {
        if self.plain.len() >= MAX_BLOCK_BYTES {
            self.truncated = true;
            return;
        }
        let decoded = decode_entities(text);
        if preformatted {
            self.plain.push_str(&decoded);
            return;
        }
        let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
        let lead = decoded.chars().next().is_some_and(char::is_whitespace)
            && !self.plain.is_empty()
            && !self.plain.ends_with(' ');
        if lead {
            self.plain.push(' ');
            self.markdown.push(' ');
        }
        if collapsed.is_empty() {
            return;
        }
        self.plain.push_str(&collapsed);
        self.markdown.push_str(&escape_markdown(&collapsed));
        if decoded.chars().last().is_some_and(char::is_whitespace) {
            self.plain.push(' ');
            self.markdown.push(' ');
        }
    }
}

pub fn parse_html(html: &str, base: &Url) -> (String, Vec<BrowserBlock>, bool) {
    let cleaned;
    let html = if html.contains(LINK_MARK) {
        cleaned = html.replace(LINK_MARK, "");
        cleaned.as_str()
    } else {
        html
    };
    let mut title = String::new();
    let mut builder = Builder {
        blocks: Vec::new(),
        kind: BrowserBlockKind::Paragraph,
        markdown: String::new(),
        plain: String::new(),
        truncated: false,
    };
    let mut position = 0;
    let mut preformatted = 0usize;
    let mut open_link = false;
    let mut in_head = false;

    while position < html.len() {
        let Some(found) = html[position..].find('<') else {
            if !in_head {
                builder.push_text(&html[position..], preformatted > 0);
            }
            break;
        };
        let tag_start = position + found;
        if tag_start > position && !in_head {
            builder.push_text(&html[position..tag_start], preformatted > 0);
        }
        if html[tag_start..].starts_with("<!--") {
            position = html[tag_start..]
                .find("-->")
                .map_or(html.len(), |end| tag_start + end + 3);
            continue;
        }
        if html[tag_start..].starts_with("<!") || html[tag_start..].starts_with("<?") {
            position = html[tag_start..]
                .find('>')
                .map_or(html.len(), |end| tag_start + end + 1);
            continue;
        }
        let Some(tag) = read_tag(html, tag_start) else {
            if !in_head {
                builder.push_text("<", preformatted > 0);
                // Keep what follows literal rather than losing it to a stray `<`.
                builder.push_text(&html[tag_start + 1..], preformatted > 0);
            }
            break;
        };
        position = tag.end;
        let name = tag.name.as_str();

        if name == "head" {
            in_head = !tag.closing;
            continue;
        }
        if name == "title" && !tag.closing {
            let lower = html[position..].to_ascii_lowercase();
            if let Some(end) = lower.find("</title") {
                if title.is_empty() {
                    title = collapsed_title(&html[position..position + end]);
                }
                position += end;
            }
            continue;
        }
        if !tag.closing && (SKIPPED.contains(&name)) && !tag.attributes.trim_end().ends_with('/') {
            let closer = format!("</{name}");
            let lower = html[position..].to_ascii_lowercase();
            position = lower.find(&closer).map_or(html.len(), |end| {
                let after = position + end;
                html[after..]
                    .find('>')
                    .map_or(html.len(), |gt| after + gt + 1)
            });
            continue;
        }
        if in_head {
            continue;
        }
        match (name, tag.closing) {
            ("h1", false) => builder.start(BrowserBlockKind::Heading1),
            ("h2", false) => builder.start(BrowserBlockKind::Heading2),
            ("h3" | "h4" | "h5" | "h6", false) => builder.start(BrowserBlockKind::Heading3),
            ("li", false) => builder.start(BrowserBlockKind::ListItem),
            ("blockquote", false) => builder.start(BrowserBlockKind::Quote),
            ("pre", false) => {
                builder.start(BrowserBlockKind::Code);
                preformatted += 1;
            }
            ("pre", true) => {
                preformatted = preformatted.saturating_sub(1);
                builder.flush();
                builder.kind = BrowserBlockKind::Paragraph;
            }
            ("hr", false) => {
                builder.flush();
                if builder.blocks.len() < MAX_BLOCKS {
                    builder.blocks.push(BrowserBlock {
                        kind: BrowserBlockKind::Rule,
                        markdown: String::new(),
                        text: String::new(),
                    });
                }
            }
            ("br", false) => {
                if preformatted > 0 {
                    builder.plain.push('\n');
                } else {
                    builder.push_text(" ", false);
                }
            }
            (
                "p" | "div" | "section" | "article" | "main" | "header" | "ul" | "ol" | "table"
                | "tr" | "td" | "th" | "dl" | "dt" | "dd" | "figure" | "figcaption" | "h1" | "h2"
                | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" | "body",
                _,
            ) => {
                builder.flush();
                builder.kind = BrowserBlockKind::Paragraph;
                open_link = false;
            }
            _ if preformatted > 0 => {}
            ("strong" | "b", _) => builder.markdown.push_str("**"),
            ("em" | "i", _) => builder.markdown.push('*'),
            ("code" | "kbd", _) => builder.markdown.push('`'),
            ("a", false) => {
                let target = attribute(tag.attributes, "href")
                    .map(decode_entities)
                    .and_then(|href| base.join(href.trim()).ok())
                    .filter(|url| matches!(url.scheme(), "http" | "https"));
                if let Some(target) = target {
                    if builder.markdown.len() < MAX_BLOCK_BYTES {
                        builder.markdown.push(LINK_MARK);
                        builder
                            .markdown
                            .push_str(&escape_link_target(target.as_str()));
                        builder.markdown.push(LINK_MARK);
                        open_link = true;
                    }
                }
            }
            ("a", true) if open_link => {
                close_link(&mut builder.markdown);
                open_link = false;
            }
            _ => {}
        }
    }
    builder.flush();
    let truncated = builder.truncated;
    (title, builder.blocks, truncated)
}

fn escape_link_target(target: &str) -> String {
    target
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20")
}

/// Rewrite the pending `<mark>target<mark>label` into `[label](target)`. A link
/// with no visible text keeps nothing.
fn close_link(markdown: &mut String) {
    let Some(second) = markdown.rfind(LINK_MARK) else {
        return;
    };
    let Some(first) = markdown[..second].rfind(LINK_MARK) else {
        return;
    };
    let target = markdown[first + LINK_MARK.len_utf8()..second].to_owned();
    let label = markdown[second + LINK_MARK.len_utf8()..].to_owned();
    markdown.truncate(first);
    if label.trim().is_empty() {
        return;
    }
    markdown.push('[');
    markdown.push_str(label.trim());
    markdown.push_str("](");
    markdown.push_str(&target);
    markdown.push(')');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.test/docs/page").expect("base url")
    }

    #[test]
    fn address_rules() {
        assert_eq!(
            normalize_address("example.com").expect("host").as_str(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_address("localhost:3000/x")
                .expect("local")
                .as_str(),
            "http://localhost:3000/x"
        );
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ftp://example.com",
            "https://user:secret@example.com",
            "",
        ] {
            assert!(
                normalize_address(refused).is_err(),
                "{refused} was accepted"
            );
        }
    }

    #[test]
    fn scripts_and_chrome_never_reach_the_page() {
        let html = r#"<html><head><title> A  page </title><style>p{}</style></head><body>
            <nav>menu</nav><script>alert("x")</script><h1>Top</h1><p>Hello <b>big</b> world</p>
            <!-- <p>hidden</p> --><footer>foot</footer></body></html>"#;
        let (title, blocks, _) = parse_html(html, &base());
        assert_eq!(title, "A page");
        let text = blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(text, ["Top", "Hello big world"]);
        assert_eq!(blocks[0].kind, BrowserBlockKind::Heading1);
        assert_eq!(blocks[1].markdown, "Hello **big** world");
    }

    #[test]
    fn links_resolve_and_unsafe_schemes_are_dropped() {
        let html = r#"<p><a href="/a">one</a> <a href="javascript:evil()">two</a> <a href="https://x.test/(y)">three</a></p>"#;
        let (_, blocks, _) = parse_html(html, &base());
        let markdown = &blocks[0].markdown;
        assert!(
            markdown.contains("[one](https://example.test/a)"),
            "{markdown}"
        );
        assert!(!markdown.contains("javascript"), "{markdown}");
        assert!(markdown.contains("two"), "{markdown}");
        assert!(
            markdown.contains("[three](https://x.test/%28y%29)"),
            "{markdown}"
        );
        assert!(!markdown.contains(LINK_MARK));
    }

    #[test]
    fn markup_in_text_is_escaped() {
        let html = "<p>a &lt;font color=red&gt;b&lt;/font&gt; *c* [d](e)</p>";
        let (_, blocks, _) = parse_html(html, &base());
        assert_eq!(blocks[0].text, "a <font color=red>b</font> *c* [d](e)");
        assert!(blocks[0].markdown.contains(r"\<font"));
        assert!(blocks[0].markdown.contains(r"\*c\*"));
        assert!(blocks[0].markdown.contains(r"\[d\]\(e\)"));
    }

    #[test]
    fn preformatted_text_keeps_its_shape() {
        let html = "<pre><code>fn main() {\n    run();\n}</code></pre><p>after</p>";
        let (_, blocks, _) = parse_html(html, &base());
        assert_eq!(blocks[0].kind, BrowserBlockKind::Code);
        assert_eq!(blocks[0].text, "fn main() {\n    run();\n}");
        assert_eq!(blocks[1].text, "after");
    }

    #[test]
    fn entities_decode_without_panicking_on_junk() {
        assert_eq!(
            decode_entities("a &amp; b &#65; &#x42; &bogus; &"),
            "a & b A B &bogus; &"
        );
        assert_eq!(decode_entities("&#xFFFFFFFF; é &"), "&#xFFFFFFFF; é &");
        let (_, blocks, _) = parse_html("<p>é<a href='x'", &base());
        assert_eq!(blocks[0].text, "é<a href='x'");
    }

    #[test]
    fn nul_bytes_in_a_page_cannot_forge_a_link() {
        let (_, blocks, _) = parse_html("<p>a\u{0}b\u{0}c</p>", &base());
        assert_eq!(blocks[0].text, "abc");
    }

    /// Answers each connection once with a fixed response, over a real socket.
    fn serve(content_type: &'static str, status: &'static str, body: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nSet-Cookie: a=b\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        format!("http://{address}/")
    }

    #[tokio::test]
    async fn a_real_page_loads_as_reader_text() {
        let url = serve(
            "text/html; charset=utf-8",
            "200 OK",
            "<title>Hi</title><script>steal()</script><h1>Top</h1><p>Body</p>",
        );
        let page = fetch(&url).await;
        assert_eq!(page.state, BrowserPageState::Ready, "{}", page.message);
        assert_eq!(page.title, "Hi");
        let text = page
            .blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(text, ["Top", "Body"]);
    }

    #[tokio::test]
    async fn binary_content_and_server_errors_are_reported_not_rendered() {
        let binary = fetch(&serve("application/octet-stream", "200 OK", "MZ")).await;
        assert_eq!(binary.state, BrowserPageState::Failed);
        assert!(binary.blocks.is_empty());
        let missing = fetch(&serve("text/html", "404 Not Found", "gone")).await;
        assert_eq!(missing.state, BrowserPageState::Failed);
        assert!(missing.message.contains("404"), "{}", missing.message);
    }

    #[tokio::test]
    async fn an_unreachable_host_fails_cleanly() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        drop(listener);
        let page = fetch(&format!("http://{address}/")).await;
        assert_eq!(page.state, BrowserPageState::Failed);
    }

    #[test]
    fn block_count_is_bounded() {
        let html = "<p>x</p>".repeat(MAX_BLOCKS + 50);
        let (_, blocks, truncated) = parse_html(&html, &base());
        assert_eq!(blocks.len(), MAX_BLOCKS);
        assert!(truncated);
    }
}
