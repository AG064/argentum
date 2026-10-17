//! Fixed A4 document profile with measured font advances and bounded pagination.
use crate::{
    markdown::{Block, Document},
    ArtifactError as Error, Result,
};
use serde::{Deserialize, Serialize};
use skrifa::{
    instance::{LocationRef, Size},
    FontRef, MetadataProvider,
};

pub const PAGE_WIDTH: f32 = 595.0;
pub const PAGE_HEIGHT: f32 = 842.0;
const LEFT: f32 = 48.0;
const TOP: f32 = 784.0;
const BOTTOM: f32 = 58.0;
const WIDTH: f32 = PAGE_WIDTH - LEFT * 2.0;
pub const MAX_PAGES: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Text {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub header: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Page {
    pub text: Vec<Text>,
    pub rectangles: Vec<Rectangle>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub pages: Vec<Page>,
}

struct Composer<'a> {
    face: FontRef<'a>,
    pages: Vec<Page>,
    y: f32,
}

impl Composer<'_> {
    fn measure(&self, text: &str, size: f32) -> Result<f32> {
        let mut width = 0f32;
        let charmap = self.face.charmap();
        let metrics = self
            .face
            .glyph_metrics(Size::unscaled(), LocationRef::default());
        let units = self
            .face
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em;
        for c in text.chars() {
            // Initial profile is horizontal precomposed Latin/Greek/Cyrillic.
            // Unsupported shaping is an error, never a missing-glyph substitute.
            if !(c == ' '
                || ('!'..='~').contains(&c)
                || ('\u{a0}'..='\u{2ff}').contains(&c)
                || ('\u{370}'..='\u{52f}').contains(&c))
            {
                return Err(Error::Contract("unsupported shaping or character in the initial PDF font profile; source was not changed"));
            }
            let glyph = charmap
                .map(c)
                .ok_or(Error::Contract("font has no glyph for source character"))?;
            let advance = metrics
                .advance_width(glyph)
                .ok_or(Error::Contract("font advance is unavailable"))?;
            if !advance.is_finite() || advance < 0.0 {
                return Err(Error::Contract("font advance is invalid"));
            }
            width += advance * size / f32::from(units);
        }
        if !width.is_finite() {
            return Err(Error::Contract("invalid measured line width"));
        }
        Ok(width)
    }
    fn wrap(&self, text: &str, width: f32, size: f32) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        for hard_line in text.split('\n') {
            let mut current = String::new();
            for word in hard_line.split(' ') {
                let candidate = if current.is_empty() {
                    word.into()
                } else {
                    format!("{current} {word}")
                };
                if self.measure(&candidate, size)? <= width {
                    current = candidate;
                    continue;
                }
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                for c in word.chars() {
                    let mut candidate = current.clone();
                    candidate.push(c);
                    if self.measure(&candidate, size)? > width {
                        if current.is_empty() {
                            return Err(Error::Contract("glyph exceeds its content area"));
                        }
                        lines.push(std::mem::take(&mut current));
                    }
                    current.push(c);
                }
            }
            lines.push(current);
        }
        if lines.len() > 2048 {
            return Err(Error::Contract("wrapped line limit exceeded"));
        }
        Ok(lines)
    }
    fn room(&mut self, height: f32) -> Result<()> {
        if height > TOP - BOTTOM {
            return Err(Error::Contract(
                "block is too tall for the supported page profile",
            ));
        }
        if self.y - height < BOTTOM {
            if self.pages.len() >= MAX_PAGES {
                return Err(Error::Contract("PDF page limit exceeded"));
            }
            self.pages.push(Page::default());
            self.y = TOP;
        }
        Ok(())
    }
    fn line(&mut self, text: String, x: f32, size: f32, height: f32) -> Result<()> {
        self.room(height)?;
        if !text.is_empty() {
            self.pages
                .last_mut()
                .ok_or(Error::Contract("layout page is missing"))?
                .text
                .push(Text {
                    x,
                    y: self.y,
                    size,
                    text,
                });
        }
        self.y -= height;
        Ok(())
    }
    fn paragraph(&mut self, text: &str, size: f32, indent: f32) -> Result<()> {
        for line in self.wrap(text, WIDTH - indent, size)? {
            self.line(line, LEFT + indent, size, size * 1.4)?;
        }
        self.y -= 8.0;
        Ok(())
    }
    fn table(&mut self, rows: &[Vec<String>]) -> Result<()> {
        let columns = rows
            .first()
            .map(Vec::len)
            .ok_or(Error::Contract("table has no rows"))?;
        if columns == 0 || columns > 4 || rows.len() > 32 || rows.iter().any(|r| r.len() != columns)
        {
            return Err(Error::Contract("unsupported table dimensions"));
        }
        let width = WIDTH / columns as f32;
        let header = self.table_lines(&rows[0], width)?;
        let header_height = header.iter().map(Vec::len).max().unwrap_or(1) as f32 * 14.0 + 12.0;
        for (i, row) in rows.iter().enumerate() {
            let lines = self.table_lines(row, width)?;
            let height = lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * 14.0 + 12.0;
            if height + header_height > TOP - BOTTOM {
                return Err(Error::Contract(
                    "table row is too tall; split its source content",
                ));
            }
            let needs_page = self.y - height < BOTTOM;
            self.room(if needs_page && i > 0 {
                height + header_height
            } else {
                height
            })?;
            if needs_page && i > 0 {
                self.draw_row(&header, width, header_height, true)?;
            }
            self.draw_row(&lines, width, height, i == 0)?;
        }
        self.y -= 12.0;
        Ok(())
    }
    fn table_lines(&self, row: &[String], width: f32) -> Result<Vec<Vec<String>>> {
        row.iter()
            .map(|s| self.wrap(s, width - 12.0, 10.0))
            .collect()
    }
    fn draw_row(
        &mut self,
        cells: &[Vec<String>],
        width: f32,
        height: f32,
        header: bool,
    ) -> Result<()> {
        let page = self
            .pages
            .last_mut()
            .ok_or(Error::Contract("layout page is missing"))?;
        for (column, lines) in cells.iter().enumerate() {
            let x = LEFT + column as f32 * width;
            page.rectangles.push(Rectangle {
                x,
                y: self.y - height + 4.0,
                width,
                height,
                header,
            });
            for (line, text) in lines.iter().enumerate() {
                if !text.is_empty() {
                    page.text.push(Text {
                        x: x + 6.0,
                        y: self.y - 8.0 - line as f32 * 14.0,
                        size: 10.0,
                        text: text.clone(),
                    });
                }
            }
        }
        self.y -= height;
        Ok(())
    }
}

pub fn compose(document: &Document, font: &[u8]) -> Result<Layout> {
    if document.schema_version != 1 || document.blocks.len() > crate::markdown::MAX_BLOCKS {
        return Err(Error::Contract("invalid layout document"));
    }
    let face = FontRef::new(font).map_err(|_| Error::Contract("pinned font is invalid"))?;
    if face
        .metrics(Size::unscaled(), LocationRef::default())
        .units_per_em
        == 0
    {
        return Err(Error::Contract("font units are invalid"));
    }
    let mut c = Composer {
        face,
        pages: vec![Page::default()],
        y: TOP,
    };
    for block in &document.blocks {
        match block {
            Block::Heading { level, text } => {
                let size = match level {
                    1 => 20.0,
                    2 => 15.0,
                    3 => 12.0,
                    _ => return Err(Error::Contract("invalid heading level")),
                };
                c.room(size * 2.8 + 10.0)?;
                c.y -= 8.0;
                c.paragraph(text, size, 0.0)?;
            }
            Block::Paragraph { text } => c.paragraph(text, 11.0, 0.0)?,
            Block::ListItem { marker, text } => {
                c.paragraph(&format!("{marker} {text}"), 11.0, 10.0)?
            }
            Block::Table { rows } => c.table(rows)?,
            Block::Code { text } => {
                for line in text.trim_end_matches('\n').split('\n') {
                    let line = line.replace('\t', "    ");
                    if c.measure(&line, 9.0)? > WIDTH - 12.0 {
                        return Err(Error::Contract(
                            "code line exceeds the page; wrap it explicitly in the Markdown source",
                        ));
                    }
                    c.line(line, LEFT + 6.0, 9.0, 13.0)?;
                }
                c.y -= 8.0;
            }
            Block::Rule => {
                c.room(14.0)?;
                c.y -= 7.0;
                c.pages
                    .last_mut()
                    .ok_or(Error::Contract("layout page missing"))?
                    .rectangles
                    .push(Rectangle {
                        x: LEFT,
                        y: c.y,
                        width: WIDTH,
                        height: 0.5,
                        header: false,
                    });
                c.y -= 7.0;
            }
        }
    }
    if c.pages.iter().any(|p| p.text.is_empty()) {
        return Err(Error::Contract("unexpected blank page"));
    }
    for (number, page) in c.pages.iter_mut().enumerate() {
        page.text.push(Text {
            x: LEFT,
            y: 30.0,
            size: 9.0,
            text: format!("{}", number + 1),
        });
    }
    Ok(Layout { pages: c.pages })
}
