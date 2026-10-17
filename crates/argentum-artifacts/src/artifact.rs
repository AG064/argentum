//! Exact local inputs and derived artifact identities. These records are not
//! execution grants, semantic grades, or native verification receipts.
use crate::{markdown, sha256, ArtifactError as Error, Result};
use argentum_execution::VerifiedFileLease;
use argentum_security::{ApprovalPolicy, CapabilityBroker};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const ORIGIN: &str = "https://local-artifacts.argentum.invalid";
pub const MAX_ARTIFACTS: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub schema_version: u16,
    pub workspace: PathBuf,
    pub inputs: Vec<Input>,
    pub markdown: String,
    pub output_directory: String,
    pub media_directory: PathBuf,
    pub worker_directory: PathBuf,
    pub worker: PathBuf,
    pub worker_sha256: String,
    pub pdfium: PathBuf,
    pub font: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tile {
    pub page: u32,
    pub tile: u32,
    pub artifact: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub y: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u16,
    pub artifact_id: Uuid,
    pub configuration_sha256: String,
    pub inputs: Vec<Input>,
    pub markdown: Input,
    pub pdf: Input,
    pub byte_count: usize,
    pub render_manifest: Input,
    pub page_count: u32,
    pub tiles: Vec<Tile>,
    pub worker_sha256: String,
    pub font_sha256: String,
    pub pdfium_sha256: String,
}

pub struct Artifacts {
    pub configuration: Configuration,
    pub context_id: Uuid,
    broker: CapabilityBroker,
    configuration_sha256: String,
    leases: Vec<VerifiedFileLease>,
}

pub fn digest_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn relative(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || value.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
        || value.len() > 512
        || value.contains(['\\', ':', '\0'])
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path
            .components()
            .any(|c| matches!(c,Component::Normal(n) if n==".git"||n=="target"))
    {
        return Err(Error::Contract("artifact paths must be explicit contained relative paths, without aliases or traversal"));
    }
    Ok(path.into())
}

pub fn read_leased(path: &Path, max: usize) -> Result<(VerifiedFileLease, Vec<u8>)> {
    let mut lease = VerifiedFileLease::open(path, max as u64).map_err(|_| Error::Io)?;
    let mut bytes = Vec::new();
    std::fs::File::open(lease.path())
        .map_err(|_| Error::Io)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > max || sha256(&bytes) != lease.sha256() {
        return Err(Error::Contract("artifact file changed during its read"));
    }
    lease
        .verify_identity()
        .and_then(|()| lease.verify_unchanged())
        .map_err(|_| Error::Io)?;
    Ok((lease, bytes))
}

fn private_directory(path: &Path) -> Result<()> {
    let acl = argentum_execution::owner_only_acl_evidence(path).map_err(|_| Error::Io)?;
    if !acl.protected
        || !acl.owner_full_control
        || acl.inherited_ace_count != 0
        || acl.non_owner_allow_ace_count != 0
    {
        return Err(Error::Contract("artifact directories must already have owner-only access; runtime does not change an existing directory's permissions"));
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| Error::Io)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| Error::Io)
}

#[cfg(windows)]
fn selection_lock(path: &Path) -> Result<std::fs::File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| {
            Error::Contract(
                "another artifact generation owns selection; no new generation was admitted",
            )
        })?;
    let metadata = file.metadata().map_err(|_| Error::Io)?;
    if metadata.file_attributes() & 0x400 != 0 || !metadata.is_file() || metadata.len() != 0 {
        return Err(Error::Contract(
            "artifact selection lock must be an unchanged empty regular file",
        ));
    }
    Ok(file)
}
#[cfg(not(windows))]
fn selection_lock(_path: &Path) -> Result<std::fs::File> {
    Err(Error::Contract(
        "artifact selection requires the Windows profile",
    ))
}

impl Configuration {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.inputs.len() != 2
            || !self.workspace.is_absolute()
            || !self.media_directory.is_absolute()
            || !self.worker_directory.is_absolute()
            || !self.worker.is_absolute()
            || !self.pdfium.is_absolute()
            || !self.font.is_absolute()
            || !digest_valid(&self.worker_sha256)
        {
            return Err(Error::Contract(
                "invalid exact local artifact configuration",
            ));
        }
        let mut names = BTreeSet::new();
        relative(&self.output_directory)?;
        relative(&self.markdown)?;
        let output = PathBuf::from(self.output_directory.to_lowercase());
        let markdown = PathBuf::from(self.markdown.to_lowercase());
        if !self.markdown.ends_with(".md") || markdown.starts_with(&output) {
            return Err(Error::Contract(
                "editable Markdown must be separate from generated artifact files",
            ));
        }
        if self.worker_directory.starts_with(&self.workspace)
            || self.media_directory.starts_with(&self.workspace)
            || self.worker_directory.starts_with(&self.media_directory)
            || self.media_directory.starts_with(&self.worker_directory)
        {
            return Err(Error::Contract(
                "private worker and media roots must be separate and outside the model workspace",
            ));
        }
        for input in &self.inputs {
            relative(&input.path)?;
            let path = PathBuf::from(input.path.to_lowercase());
            if !digest_valid(&input.sha256)
                || !names.insert(path.clone())
                || path == markdown
                || path.starts_with(&output)
            {
                return Err(Error::Contract(
                    "source identities, editable source and generated output must be distinct",
                ));
            }
        }
        Ok(())
    }
}

impl Artifacts {
    pub fn load(configuration_file: &Path) -> Result<Self> {
        let (lease, bytes) = read_leased(configuration_file, 16 * 1024)?;
        let config: Configuration = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Contract("invalid artifact configuration JSON"))?;
        config.validate()?;
        private_directory(&config.media_directory)?;
        private_directory(&config.worker_directory)?;
        let broker = CapabilityBroker::new(&config.workspace, ApprovalPolicy::default())
            .map_err(|_| Error::Io)?;
        if broker.root() != config.workspace.canonicalize().map_err(|_| Error::Io)? {
            return Err(Error::Contract("workspace identity changed"));
        }
        let mut leases = vec![lease];
        for (path, expected, max) in [
            (
                &config.worker,
                config.worker_sha256.as_str(),
                64 * 1024 * 1024,
            ),
            (
                &config.pdfium,
                argentum_execution::PDFIUM_DLL_SHA256,
                16 * 1024 * 1024,
            ),
            (
                &config.font,
                argentum_execution::ARTIFACT_FONT_SHA256,
                4 * 1024 * 1024,
            ),
        ] {
            let lease = VerifiedFileLease::open(path, max).map_err(|_| Error::Io)?;
            if lease.sha256() != expected {
                return Err(Error::Contract(
                    "artifact worker/font/parser binding changed",
                ));
            }
            leases.push(lease);
        }
        let result = Self {
            configuration: config,
            context_id: Uuid::new_v4(),
            broker,
            configuration_sha256: sha256(&bytes),
            leases,
        };
        result.check_inputs()?;
        Ok(result)
    }

    pub fn check_inputs(&self) -> Result<()> {
        for lease in &self.leases {
            lease.verify_identity().map_err(|_| Error::Io)?;
        }
        for input in &self.configuration.inputs {
            let bytes = self.read_workspace(&input.path, 64 * 1024)?;
            if sha256(&bytes) != input.sha256 {
                return Err(Error::Contract(
                    "approved source document changed; downstream artifacts are unverified",
                ));
            }
            std::str::from_utf8(&bytes)
                .map_err(|_| Error::Contract("approved document is not UTF-8"))?;
        }
        Ok(())
    }

    fn read_workspace(&self, path: &str, max: usize) -> Result<Vec<u8>> {
        relative(path)?;
        let (_, file) = self
            .broker
            .open_file_for_read(path)
            .map_err(|_| Error::Io)?;
        let mut bytes = Vec::new();
        file.take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        if bytes.len() > max {
            return Err(Error::Contract(
                "artifact source exceeds the declared byte bound",
            ));
        }
        Ok(bytes)
    }

    pub fn open(&self) -> Result<Value> {
        self.check_inputs()?;
        let current = self.current()?;
        let source = match self.broker.open_file_for_read(&self.configuration.markdown) {
            Ok((_, file)) => {
                let mut bytes = Vec::new();
                file.take(markdown::MAX_MARKDOWN_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| Error::Io)?;
                if bytes.len() > markdown::MAX_MARKDOWN_BYTES {
                    return Err(Error::Contract("editable Markdown exceeds its byte bound"));
                }
                Some(sha256(&bytes))
            }
            Err(argentum_security::SecurityError::WorkspaceOperation { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                None
            }
            Err(_) => return Err(Error::Io),
        };
        Ok(
            json!({"context_id":current.as_ref().map(|r|r.artifact_id).unwrap_or(self.context_id),"origin":ORIGIN,"inputs":self.configuration.inputs,
            "markdown_path":self.configuration.markdown,"markdown_sha256":source,
            "current_artifact":current.map(|r|r.artifact_id),"output_directory":self.configuration.output_directory,
            "profile":"headings 1-3, paragraphs, flat lists, rectangular tables up to 4 columns, literal code; precomposed Latin/Greek/Cyrillic; no HTML, links, images, executable templates or inline emphasis",
            "guidance":"Read the two sources with normal tools. Author or correct Markdown through normal guarded edits. Render the current SHA, then inspect every returned page tile. Run current native verification and acknowledge its actual receipt before completion."}),
        )
    }

    pub fn current(&self) -> Result<Option<Record>> {
        let marker = format!("{}/current.json", self.configuration.output_directory);
        match self.broker.open_file_for_read(&marker) {
            Ok((_, file)) => {
                let mut bytes = Vec::new();
                file.take(64 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| Error::Io)?;
                if bytes.len() > 64 * 1024 {
                    return Err(Error::Contract("artifact selection exceeds its bound"));
                }
                let record: Record = serde_json::from_slice(&bytes)
                    .map_err(|_| Error::Contract("invalid artifact selection"))?;
                self.validate_record(&record)?;
                Ok(Some(record))
            }
            Err(argentum_security::SecurityError::WorkspaceOperation { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(_) => Err(Error::Io),
        }
    }

    fn validate_record(&self, r: &Record) -> Result<()> {
        if r.schema_version != 1
            || r.configuration_sha256 != self.configuration_sha256
            || r.inputs != self.configuration.inputs
            || r.markdown.path != self.configuration.markdown
            || !digest_valid(&r.markdown.sha256)
            || !digest_valid(&r.pdf.sha256)
            || !digest_valid(&r.render_manifest.sha256)
            || r.pdf.path
                != format!(
                    "{}/{}/document.pdf",
                    self.configuration.output_directory, r.artifact_id
                )
            || r.render_manifest.path
                != format!(
                    "{}/{}/render.json",
                    self.configuration.output_directory, r.artifact_id
                )
            || r.worker_sha256 != self.configuration.worker_sha256
            || r.font_sha256 != argentum_execution::ARTIFACT_FONT_SHA256
            || r.pdfium_sha256 != argentum_execution::PDFIUM_DLL_SHA256
            || r.byte_count > 10 * 1024 * 1024
            || r.page_count == 0
            || r.page_count > 16
            || r.tiles.is_empty()
            || r.tiles.len() > 64
        {
            return Err(Error::Contract(
                "artifact record differs from its admitted inputs/profile/output scope",
            ));
        }
        let mut identities = BTreeSet::new();
        for tile in &r.tiles {
            if tile
                .artifact
                .strip_suffix(".png")
                .and_then(|s| Uuid::parse_str(s).ok())
                .is_none()
                || !digest_valid(&tile.sha256)
                || tile.page == 0
                || tile.page > r.page_count
                || tile.tile == 0
                || tile.tile > 5
                || tile.width != 1000
                || tile.height == 0
                || tile.height > 800
                || tile.y > 3000
                || tile.y.saturating_add(tile.height) > 3000
                || !identities.insert((tile.page, tile.tile))
            {
                return Err(Error::Contract("invalid bounded page-image identity"));
            }
        }
        Ok(())
    }

    pub async fn render(
        &self,
        source_sha: &str,
        expected: Option<Uuid>,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        self.check_inputs()?;
        let current = self.current()?;
        if current.as_ref().map(|r| r.artifact_id) != expected {
            return Err(Error::Contract(
                "artifact selection changed; open again and submit a new render",
            ));
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let (source_lease, source) = read_leased(
            &self
                .configuration
                .workspace
                .join(relative(&self.configuration.markdown)?),
            markdown::MAX_MARKDOWN_BYTES,
        )?;
        if sha256(&source) != source_sha {
            return Err(Error::Contract(
                "stale Markdown SHA; read the current source before a new render",
            ));
        }
        let text =
            std::str::from_utf8(&source).map_err(|_| Error::Contract("Markdown is not UTF-8"))?;
        markdown::parse(text)?;
        let output = self
            .configuration
            .workspace
            .join(relative(&self.configuration.output_directory)?);
        if !output.exists() {
            argentum_execution::ensure_owner_only_directory(&output).map_err(|_| Error::Io)?;
        }
        private_directory(&output)?;
        // A file handle, not a thread-affine mutex, stays owned across awaits.
        // Share-none serializes selection across producer processes. Crashes
        // release the handle; the empty file remains, without replaying work.
        let _selection_lock = selection_lock(&output.join("selection.lock"))?;
        if self.current()?.as_ref().map(|r| r.artifact_id) != expected {
            return Err(Error::Contract(
                "artifact selection changed before generation admission",
            ));
        }
        let made = argentum_execution::run_isolated_markdown_worker(
            &self.configuration.worker,
            &self.configuration.worker_sha256,
            &self.configuration.pdfium,
            &self.configuration.font,
            text,
            &self.configuration.worker_directory,
            cancel,
        )
        .await
        .map_err(|_| Error::Io)?;
        if made.exit_code != Some(0) || made.timed_out {
            return Err(Error::Contract("PDF creation failed; check the supported syntax, Unicode and layout bounds; no artifact is verified"));
        }
        let (_, pdf) = read_leased(&made.directory.join("output/result.pdf"), 10 * 1024 * 1024)?;
        let rendered = argentum_execution::run_isolated_pdf_worker(
            &self.configuration.worker,
            &self.configuration.worker_sha256,
            &self.configuration.pdfium,
            &pdf,
            &self.configuration.worker_directory,
            cancel,
        )
        .await
        .map_err(|_| Error::Io)?;
        if rendered.exit_code != Some(0) || rendered.timed_out {
            return Err(Error::Contract(
                "PDF parsing/rendering failed; no artifact is verified",
            ));
        }
        let (_, manifest) =
            read_leased(&rendered.directory.join("output/render.json"), 1024 * 1024)?;
        let report: Value = serde_json::from_slice(&manifest)
            .map_err(|_| Error::Contract("invalid renderer result"))?;
        if report["pdf_sha256"] != sha256(&pdf) {
            return Err(Error::Contract("rendered PDF identity differs"));
        }
        self.check_inputs()?;
        source_lease.verify_identity().map_err(|_| Error::Io)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut generations = 0;
        for entry in std::fs::read_dir(&output).map_err(|_| Error::Io)? {
            let entry = entry.map_err(|_| Error::Io)?;
            if entry.file_type().map_err(|_| Error::Io)?.is_dir() {
                generations += 1;
            }
        }
        if generations >= MAX_ARTIFACTS {
            return Err(Error::Contract("four retained artifact generations reached; no automatic deletion or allowance increase"));
        }
        if self.current()?.as_ref().map(|r| r.artifact_id) != expected {
            return Err(Error::Contract(
                "artifact selection changed during rendering",
            ));
        }
        let id = Uuid::new_v4();
        let directory = output.join(id.to_string());
        if directory.exists() {
            return Err(Error::Contract("artifact identity already exists"));
        }
        argentum_execution::ensure_owner_only_directory(&directory).map_err(|_| Error::Io)?;
        write_new(&directory.join("document.pdf"), &pdf)?;
        write_new(&directory.join("render.json"), &manifest)?;
        let pages = report["pages"]
            .as_array()
            .ok_or(Error::Contract("render pages are missing"))?;
        if pages.is_empty() || pages.len() > 16 {
            return Err(Error::Contract("invalid page count"));
        }
        let mut tiles = Vec::new();
        for (page_index, page) in pages.iter().enumerate() {
            let page_tiles = page["tiles"]
                .as_array()
                .ok_or(Error::Contract("render tiles are missing"))?;
            if page_tiles.is_empty() || page_tiles.len() > 5 || tiles.len() + page_tiles.len() > 64
            {
                return Err(Error::Contract("render image count exceeds its bound"));
            }
            for (tile_index, tile) in page_tiles.iter().enumerate() {
                let name = tile["artifact"]
                    .as_str()
                    .ok_or(Error::Contract("tile identity missing"))?;
                if name
                    .strip_suffix(".png")
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .is_none()
                {
                    return Err(Error::Contract("invalid tile identity"));
                }
                let (_, png) =
                    read_leased(&rendered.directory.join("output").join(name), 128 * 1024)?;
                if tile["sha256"] != sha256(&png) {
                    return Err(Error::Contract("rendered tile changed"));
                }
                write_new(&self.configuration.media_directory.join(name), &png)?;
                tiles.push(Tile {
                    page: page_index as u32 + 1,
                    tile: tile_index as u32 + 1,
                    artifact: name.into(),
                    sha256: sha256(&png),
                    width: tile["width"]
                        .as_u64()
                        .ok_or(Error::Contract("tile width missing"))?
                        as u32,
                    height: tile["height"]
                        .as_u64()
                        .ok_or(Error::Contract("tile height missing"))?
                        as u32,
                    y: tile["y"]
                        .as_u64()
                        .ok_or(Error::Contract("tile offset missing"))?
                        as u32,
                });
            }
        }
        if tiles.len() > 64 {
            return Err(Error::Contract(
                "image count exceeds the local artifact profile",
            ));
        }
        let record = Record {
            schema_version: 1,
            artifact_id: id,
            configuration_sha256: self.configuration_sha256.clone(),
            inputs: self.configuration.inputs.clone(),
            markdown: Input {
                path: self.configuration.markdown.clone(),
                sha256: source_sha.into(),
            },
            pdf: Input {
                path: format!("{}/{id}/document.pdf", self.configuration.output_directory),
                sha256: sha256(&pdf),
            },
            byte_count: pdf.len(),
            render_manifest: Input {
                path: format!("{}/{id}/render.json", self.configuration.output_directory),
                sha256: sha256(&manifest),
            },
            page_count: pages.len() as u32,
            tiles,
            worker_sha256: self.configuration.worker_sha256.clone(),
            font_sha256: argentum_execution::ARTIFACT_FONT_SHA256.into(),
            pdfium_sha256: argentum_execution::PDFIUM_DLL_SHA256.into(),
        };
        self.validate_record(&record)?;
        let bytes = serde_json::to_vec_pretty(&record).map_err(|_| Error::Io)?;
        write_new(&directory.join("artifact.json"), &bytes)?;
        let pending = directory.join("selection.new.json");
        write_new(&pending, &bytes)?;
        self.check_inputs()?;
        source_lease.verify_identity().map_err(|_| Error::Io)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.current()?.as_ref().map(|r| r.artifact_id) != expected {
            return Err(Error::Contract(
                "selection changed; new generation retained but not selected",
            ));
        }
        replace_selection(&pending, &output.join("current.json"))?;
        Ok(
            json!({"context_id":id,"origin":ORIGIN,"artifact_id":id,"markdown":record.markdown,
            "pdf":record.pdf,"page_count":record.page_count,"tiles":record.tiles,
            "verification":"not yet verified; inspect all page tiles and run the current native verifier"}),
        )
    }

    pub fn require_current(&self, id: Uuid) -> Result<Record> {
        self.check_inputs()?;
        let record = self
            .current()?
            .filter(|r| r.artifact_id == id)
            .ok_or(Error::Contract("artifact is not the current selection"))?;
        if sha256(&self.read_workspace(&self.configuration.markdown, markdown::MAX_MARKDOWN_BYTES)?)
            != record.markdown.sha256
            || sha256(&self.read_workspace(&record.pdf.path, 10 * 1024 * 1024)?)
                != record.pdf.sha256
            || sha256(&self.read_workspace(&record.render_manifest.path, 1024 * 1024)?)
                != record.render_manifest.sha256
        {
            return Err(Error::Contract("Markdown, PDF or render record changed; export and inspect again before verification"));
        }
        Ok(record)
    }

    pub fn page(&self, id: Uuid, page: u32, tile: u32) -> Result<Value> {
        let record = self.require_current(id)?;
        let image = record
            .tiles
            .iter()
            .find(|t| t.page == page && t.tile == tile)
            .ok_or(Error::Contract(
                "choose a page/tile from the current render result",
            ))?;
        let (_, png) = read_leased(
            &self.configuration.media_directory.join(&image.artifact),
            128 * 1024,
        )?;
        if sha256(&png) != image.sha256 {
            return Err(Error::Contract("page image changed; render again"));
        }
        let directory = self
            .configuration
            .workspace
            .join(relative(&self.configuration.output_directory)?)
            .join(id.to_string());
        let prepared = directory.join(format!(
            "prepared-{}-{}.json",
            id,
            image.artifact.trim_end_matches(".png")
        ));
        let evidence = json!({"schema_version":1,"context_id":id,"artifact_id":id,"pdf_sha256":record.pdf.sha256,
            "png_sha256":image.sha256,"page":page,"tile":tile,
            "disposition":"producer preparation only; not transport acceptance or visual comprehension"});
        if !prepared.exists() {
            write_new(
                &prepared,
                &serde_json::to_vec(&evidence).map_err(|_| Error::Io)?,
            )?;
        }
        Ok(
            json!({"context_id":id,"origin":ORIGIN,"artifact_id":id,"pdf_sha256":record.pdf.sha256,
            "artifact":image.artifact,"sha256":image.sha256,"page":page,"tile":tile,
            "width":image.width,"height":image.height,"y":image.y,"image_preparation":"actual PNG; input acceptance and visual comprehension are separate"}),
        )
    }
}

#[cfg(windows)]
fn replace_selection(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: only private product-generated metadata paths, after explicit
    // expected-selection checks. Earlier generations remain retained.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(Error::Io);
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace_selection(_from: &Path, _to: &Path) -> Result<()> {
    Err(Error::Contract("local artifact profile requires Windows"))
}
