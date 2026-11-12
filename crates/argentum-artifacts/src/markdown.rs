//! A deliberately small Markdown-to-layout compiler, not another edit format.
//! Unsupported content is rejected explicitly. Nothing fetches links or executes
//! raw HTML, templates, code blocks or document instructions.
use crate::{ArtifactError as Error, Result};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

pub const MAX_MARKDOWN_BYTES: usize = 64 * 1024;
pub const MAX_BLOCKS: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    ListItem { marker: String, text: String },
    Table { rows: Vec<Vec<String>> },
    Code { text: String },
    Rule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema_version: u16,
    pub blocks: Vec<Block>,
}

pub fn parse(markdown: &str) -> Result<Document> {
    if markdown.len() > MAX_MARKDOWN_BYTES
        || markdown.trim().is_empty()
        || markdown
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(Error::Contract(
            "Markdown must be nonempty bounded UTF-8 without control characters",
        ));
    }
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES);
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut heading = None;
    let mut code = false;
    let mut list: Option<Option<u64>> = None;
    let mut in_item = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for event in parser {
        match event {
            Event::Start(tag)=>match tag {
                Tag::Heading{level,..} if !in_item && !in_table && !code => {
                    if !text.is_empty() {return Err(Error::Contract("unexpected heading nesting"));}
                    heading=Some(match level {HeadingLevel::H1=>1,HeadingLevel::H2=>2,HeadingLevel::H3=>3,_=>return Err(Error::Contract("only heading levels 1 through 3 are supported"))});
                },
                Tag::Paragraph=>(),
                Tag::List(start) if list.is_none() && !in_table=>list=Some(start),
                Tag::Item if list.is_some() && !in_item=>{in_item=true;},
                Tag::Table(alignments) if list.is_none() && !in_table && alignments.len()<=4=>{in_table=true;rows.clear();},
                Tag::TableHead|Tag::TableRow if in_table=>row.clear(),
                Tag::TableCell if in_table && !in_cell=>{in_cell=true;text.clear();},
                Tag::CodeBlock(_) if list.is_none() && !in_table=>code=true,
                Tag::Emphasis|Tag::Strong|Tag::Strikethrough=>return Err(Error::Contract("inline emphasis is unsupported in this initial PDF profile; use headings or plain text")),
                _=>return Err(Error::Contract("unsupported Markdown structure; use headings, paragraphs, one-level lists, simple tables or literal code")),
            },
            Event::End(tag)=>match tag {
                TagEnd::Heading(_)=>{
                    blocks.push(Block::Heading{level:heading.take().ok_or(Error::Contract("invalid heading"))?,text:std::mem::take(&mut text)});
                },
                TagEnd::Paragraph if !in_item && !in_cell=>{
                    if !text.is_empty(){blocks.push(Block::Paragraph{text:std::mem::take(&mut text)});}
                },
                TagEnd::Paragraph if in_item=>{if !text.ends_with('\n'){text.push('\n');}},
                TagEnd::Item=>{
                    let marker=match list.as_mut(){Some(Some(n))=>{let s=format!("{n}.");*n=n.checked_add(1).ok_or(Error::Contract("list number overflow"))?;s},_=>"-".into()};
                    blocks.push(Block::ListItem{marker,text:std::mem::take(&mut text).trim_end_matches('\n').into()});in_item=false;
                },
                TagEnd::List(_)=>list=None,
                TagEnd::TableCell=>{row.push(std::mem::take(&mut text));in_cell=false;},
                TagEnd::TableHead|TagEnd::TableRow=>{
                    if row.is_empty()||row.len()>4||rows.len()>=32{return Err(Error::Contract("table must have at most 32 rows and 4 columns"));}
                    rows.push(std::mem::take(&mut row));
                },
                TagEnd::Table=>{
                    if rows.is_empty()||rows.iter().any(|r|r.len()!=rows[0].len()){return Err(Error::Contract("table must be rectangular"));}
                    blocks.push(Block::Table{rows:std::mem::take(&mut rows)});in_table=false;
                },
                TagEnd::CodeBlock=>{blocks.push(Block::Code{text:std::mem::take(&mut text)});code=false;},
                TagEnd::Emphasis|TagEnd::Strong|TagEnd::Strikethrough|TagEnd::Paragraph=>(),
                _=>return Err(Error::Contract("unsupported Markdown closing structure")),
            },
            Event::Text(value)|Event::Code(value)=>text.push_str(&value),
            Event::SoftBreak=>text.push(' '),
            Event::HardBreak=>text.push('\n'),
            Event::Rule=>blocks.push(Block::Rule),
            Event::Html(_)|Event::InlineHtml(_)=>return Err(Error::Contract("raw HTML is unsupported; no HTML was executed or flattened")),
            _=>return Err(Error::Contract("images, links, math, footnotes and task controls are unsupported in the initial PDF profile")),
        }
        if blocks.len() > MAX_BLOCKS || text.len() > MAX_MARKDOWN_BYTES {
            return Err(Error::Contract("Markdown structure limit exceeded"));
        }
    }
    if blocks.is_empty() || !text.is_empty() || in_table || in_item || code || list.is_some() {
        return Err(Error::Contract("incomplete Markdown document"));
    }
    Ok(Document {
        schema_version: 1,
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headings_tables_unicode_lists_and_literal_code_are_structured() {
        let parsed=parse("# Handover\n\nRésumé from Pärnu.\n\n| Fact | Value |\n|---|---|\n| Owner | Õun |\n\n- Preserve sources\n- Verify PDF\n\n```text\nnot executable\n```\n").unwrap();
        assert!(matches!(&parsed.blocks[0],Block::Heading{level:1,text} if text=="Handover"));
        assert!(parsed
            .blocks
            .iter()
            .any(|b| matches!(b,Block::Table{rows} if rows.len()==2&&rows[1][1]=="Õun")));
        assert_eq!(
            parsed
                .blocks
                .iter()
                .filter(|b| matches!(b, Block::ListItem { .. }))
                .count(),
            2
        );
        assert!(parsed
            .blocks
            .iter()
            .any(|b| matches!(b,Block::Code{text} if text=="not executable\n")));
    }
    #[test]
    fn unsupported_content_is_not_silently_lost() {
        for source in [
            "<script>alert(1)</script>",
            "[link](https://example.invalid)",
            "![image](file:///private)",
            "> quoted block",
            "#### unsupported heading",
            "- item\n  - nested item",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }
    #[test]
    fn bounded_input_and_structure() {
        assert!(parse("").is_err());
        assert!(parse(&"x".repeat(MAX_MARKDOWN_BYTES + 1)).is_err());
        assert!(parse("bad\u{0000}data").is_err());
        assert!(parse(&"paragraph\n\n".repeat(MAX_BLOCKS + 1)).is_err());
    }
}
