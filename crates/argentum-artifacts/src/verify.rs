//! Fixed native verifier: raw-byte consistency, source preservation and producer
//! preparation coverage. Semantic and visual acceptance remain independent.
use crate::{
    artifact::{read_leased, relative, Artifacts},
    layout,
    markdown::{self, Block},
    sha256, ArtifactError as Error, Result,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tokio_util::sync::CancellationToken;

fn tokens(text: &str) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    for token in text.split_whitespace() {
        *result.entry(token.into()).or_default() += 1;
    }
    result
}
fn add(result: &mut BTreeMap<String, usize>, text: &str) {
    for (token, count) in tokens(text) {
        *result.entry(token).or_default() += count;
    }
}

pub async fn verify(configuration: &Path, cancel: &CancellationToken) -> Result<Value> {
    let artifacts = Artifacts::load(configuration)?;
    let selected = artifacts
        .current()?
        .ok_or(Error::Contract("no selected artifact"))?;
    let record = artifacts.require_current(selected.artifact_id)?;
    let root = &artifacts.configuration.workspace;
    let (markdown_lease, source) = read_leased(
        &root.join(relative(&record.markdown.path)?),
        markdown::MAX_MARKDOWN_BYTES,
    )?;
    let text =
        std::str::from_utf8(&source).map_err(|_| Error::Contract("Markdown is not UTF-8"))?;
    let document = markdown::parse(text)?;
    let (pdf_lease, pdf) = read_leased(&root.join(relative(&record.pdf.path)?), 10 * 1024 * 1024)?;
    let renderer = argentum_execution::run_isolated_pdf_worker(
        &artifacts.configuration.worker,
        &artifacts.configuration.worker_sha256,
        &artifacts.configuration.pdfium,
        &pdf,
        &artifacts.configuration.worker_directory,
        cancel,
    )
    .await
    .map_err(|_| Error::Io)?;
    if renderer.exit_code != Some(0) || renderer.timed_out {
        return Err(Error::Contract(
            "current raw PDF failed isolated verification",
        ));
    }
    let (_, fresh) = read_leased(&renderer.directory.join("output/render.json"), 1024 * 1024)?;
    let fresh: Value = serde_json::from_slice(&fresh)
        .map_err(|_| Error::Contract("invalid fresh render manifest"))?;
    if fresh["pdf_sha256"] != record.pdf.sha256 || fresh["page_count"] != record.page_count {
        return Err(Error::Contract("fresh PDF identity/page count differs"));
    }
    let mut actual = BTreeMap::new();
    let mut actual_characters = BTreeMap::new();
    for page in fresh["pages"]
        .as_array()
        .ok_or(Error::Contract("fresh pages are missing"))?
    {
        add(
            &mut actual,
            page["text"]
                .as_str()
                .ok_or(Error::Contract("fresh PDF text unavailable"))?,
        );
        for c in page["text"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_whitespace())
        {
            *actual_characters.entry(c).or_insert(0usize) += 1;
        }
        let number = page["number"]
            .as_u64()
            .ok_or(Error::Contract("fresh page identity missing"))?;
        for (index, tile) in page["tiles"]
            .as_array()
            .ok_or(Error::Contract("fresh tiles missing"))?
            .iter()
            .enumerate()
        {
            if !record.tiles.iter().any(|old| {
                u64::from(old.page) == number
                    && old.tile == index as u32 + 1
                    && tile["sha256"] == old.sha256
                    && tile["width"] == old.width
                    && tile["height"] == old.height
                    && tile["y"] == old.y
            }) {
                return Err(Error::Contract(
                    "fresh render pixels differ from the prepared artifact",
                ));
            }
        }
    }
    let mut source_characters = BTreeMap::new();
    let mut source_text = String::new();
    for block in &document.blocks {
        match block {
            Block::Heading { text, .. } | Block::Paragraph { text } | Block::Code { text } => {
                source_text.push_str(text)
            }
            Block::ListItem { marker, text } => {
                source_text.push_str(marker);
                source_text.push_str(text);
            }
            Block::Table { rows } => {
                for row in rows {
                    for cell in row {
                        source_text.push_str(cell);
                    }
                }
            }
            Block::Rule => (),
        }
    }
    for c in source_text.chars().filter(|c| !c.is_whitespace()) {
        *source_characters.entry(c).or_insert(0usize) += 1;
    }
    if source_characters
        .iter()
        .any(|(c, n)| actual_characters.get(c).copied().unwrap_or(0) < *n)
    {
        return Err(Error::Contract("current PDF omits source text characters"));
    }
    // Reuse the fixed layout to account for soft-wrapped SHA strings, page
    // footers and repeated table headers. This is mechanical text inventory,
    // not independent semantic/layout proof; the original-source coverage above
    // remains separate from the creator's layout model.
    let (_, font) = read_leased(&artifacts.configuration.font, 4 * 1024 * 1024)?;
    let expected_layout = layout::compose(&document, &font)?;
    let mut expected = BTreeMap::new();
    for page in &expected_layout.pages {
        for line in &page.text {
            add(&mut expected, &line.text);
        }
    }
    if actual != expected {
        return Err(Error::Contract(
            "current PDF text inventory differs from the fixed source layout",
        ));
    }
    let directory = root
        .join(relative(&artifacts.configuration.output_directory)?)
        .join(record.artifact_id.to_string());
    let mut prepared = BTreeSet::new();
    let mut count = 0;
    for entry in std::fs::read_dir(&directory).map_err(|_| Error::Io)? {
        count += 1;
        if count > 1024 {
            return Err(Error::Contract("artifact evidence count exceeds its bound"));
        }
        let entry = entry.map_err(|_| Error::Io)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("prepared-") || !name.ends_with(".json") {
            continue;
        }
        let (_, bytes) = read_leased(&entry.path(), 4096)?;
        let evidence: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Contract("invalid image preparation record"))?;
        for tile in &record.tiles {
            if evidence["artifact_id"] == record.artifact_id.to_string()
                && evidence["pdf_sha256"] == record.pdf.sha256
                && evidence["png_sha256"] == tile.sha256
                && evidence["page"] == tile.page
                && evidence["tile"] == tile.tile
            {
                let (_, png) = read_leased(
                    &artifacts.configuration.media_directory.join(&tile.artifact),
                    128 * 1024,
                )?;
                if sha256(&png) != tile.sha256 {
                    return Err(Error::Contract("prepared image changed after selection"));
                }
                prepared.insert((tile.page, tile.tile));
            }
        }
    }
    if prepared.len() != record.tiles.len() {
        return Err(Error::Contract(
            "not every current page tile was prepared for image input",
        ));
    }
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    artifacts.require_current(record.artifact_id)?;
    markdown_lease.verify_identity().map_err(|_| Error::Io)?;
    pdf_lease.verify_identity().map_err(|_| Error::Io)?;
    Ok(
        json!({"schema_version":1,"artifact_id":record.artifact_id,"markdown":record.markdown,"pdf":record.pdf,
        "page_count":record.page_count,"producer_prepared_tiles":prepared.len(),"source_preservation":"passed",
        "raw_pdf_parse_and_render":"passed","fresh_pixels_match":"passed","text_token_inventory":"passed",
        "not_established":["natural-language task correctness","table fact associations","layout acceptance","model transport acceptance","model visual comprehension"],
        "native_verification":"when invoked by the approved runner, the runtime supplies the authoritative current receipt"}),
    )
}
