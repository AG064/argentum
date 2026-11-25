// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The model marketplace's Hugging Face Hub client: search GGUF model
//! repositories, list the GGUF files in one, and download a chosen file into the
//! local model folder.
//!
//! Only the public Hub API is used and no account is needed. A Hugging Face
//! token, which only gated models need, is sent to the Hub's own host and never
//! to the storage host a download redirects to (reqwest drops the header when a
//! redirect changes host). The token type never prints its value.
//!
//! Every download is pinned to the repository commit that was listed and checked
//! against the SHA-256 the Hub publishes for that file, its Git LFS object id.
//! An interrupted download keeps its partial file next to the target, with a
//! small record of what it is, and the next download of the same file resumes
//! with an HTTP range request. A partial file whose record does not match is
//! started over. A file that fails its checksum is deleted.
//!
//! Nothing here runs in the background. Each request starts from an explicit
//! action in Settings, and nothing about the machine is sent.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::UpdateError;

/// The public Hugging Face Hub.
pub const HUB_URL: &str = "https://huggingface.co";
/// Largest search or repository answer read, so a hostile answer cannot fill memory.
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
/// Search results asked for at once.
pub const SEARCH_LIMIT: usize = 30;
/// Free space kept on the drive after a download, beyond the files themselves.
pub const DISK_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;
const PARTIAL_SUFFIX: &str = ".download";
const PARTIAL_RECORD_SUFFIX: &str = ".download.json";

fn fail<T>(message: impl Into<String>) -> Result<T, UpdateError> {
    Err(UpdateError(message.into()))
}

/// A Hugging Face access token. Its value is never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct HubToken(String);

impl HubToken {
    /// `None` for an empty or malformed value.
    pub fn new(value: &str) -> Option<Self> {
        let value = value.trim();
        (!value.is_empty()
            && value.len() <= 512
            && value.bytes().all(|byte| byte.is_ascii_graphic()))
        .then(|| Self(value.to_owned()))
    }
}

impl std::fmt::Debug for HubToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HubToken(hidden)")
    }
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSummary {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub license: Option<String>,
    pub gated: bool,
}

/// Whether a GGUF file is a model or a vision projector for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantKind {
    Model,
    Projector,
}

/// One file in a repository, at the listed commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    /// The path inside the repository, possibly in a folder.
    pub path: String,
    /// The name it gets in the local model folder.
    pub local_name: String,
    pub size_bytes: u64,
    /// The Hub's published SHA-256, the file's Git LFS object id.
    pub sha256: String,
}

/// A quantization of the model: one GGUF file, or the parts of a split one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// The quantization name, such as `Q4_K_M`, when the file name gives one.
    pub quant: Option<String>,
    pub kind: VariantKind,
    /// In part order. The first part is the one the runtime opens.
    pub files: Vec<RepoFile>,
}

impl Variant {
    pub fn size_bytes(&self) -> u64 {
        self.files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.size_bytes))
    }

    /// The local file name the runtime is pointed at.
    pub fn model_file(&self) -> &str {
        self.files
            .first()
            .map_or("", |file| file.local_name.as_str())
    }

    pub fn label(&self) -> String {
        let name = self
            .quant
            .clone()
            .unwrap_or_else(|| self.model_file().into());
        match (self.kind, self.files.len()) {
            (VariantKind::Projector, _) => format!("{name} vision projector"),
            (VariantKind::Model, 1) => name,
            (VariantKind::Model, parts) => format!("{name}, {parts} parts"),
        }
    }
}

/// A repository and its downloadable GGUF files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub id: String,
    /// The commit every download is pinned to.
    pub commit: String,
    pub license: Option<String>,
    pub gated: bool,
    pub context_length: Option<u64>,
    pub architecture: Option<String>,
    pub parameters: Option<u64>,
    pub variants: Vec<Variant>,
    /// GGUF files left out because the Hub publishes no SHA-256 for them, or
    /// because their name or split set is not one Argentum accepts.
    pub skipped_files: Vec<String>,
}

impl Repo {
    pub fn page_url(&self) -> String {
        format!("{HUB_URL}/{}", self.id)
    }
}

#[derive(Deserialize)]
struct WireSummary {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    gated: serde_json::Value,
}

#[derive(Deserialize)]
struct WireRepo {
    id: String,
    sha: String,
    #[serde(default)]
    gated: serde_json::Value,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default, rename = "cardData")]
    card_data: Option<WireCard>,
    #[serde(default)]
    gguf: Option<WireGguf>,
    #[serde(default)]
    siblings: Vec<WireSibling>,
}

#[derive(Deserialize)]
struct WireCard {
    #[serde(default)]
    license: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct WireGguf {
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    context_length: Option<u64>,
}

#[derive(Deserialize)]
struct WireSibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    lfs: Option<WireLfs>,
}

#[derive(Deserialize)]
struct WireLfs {
    sha256: String,
    size: u64,
}

fn is_gated(value: &serde_json::Value) -> bool {
    !matches!(
        value,
        serde_json::Value::Null | serde_json::Value::Bool(false)
    )
}

fn license_from_tags(tags: &[String]) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag.strip_prefix("license:"))
        .map(clean_text)
}

/// Text from the Hub is shown in the interface, so keep it short and printable.
fn clean_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect()
}

/// `owner/name`, in the characters the Hub allows.
pub fn valid_repo_id(id: &str) -> bool {
    let mut parts = id.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    id.len() <= 96
        && [owner, name].iter().all(|part| {
            !part.is_empty()
                && !part.starts_with(['.', '-'])
                && !part.contains("..")
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn valid_commit(commit: &str) -> bool {
    commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The local name for a repository path, or `None` if the path could escape
/// the model folder or the name is not a plain GGUF file name.
fn local_name(path: &str) -> Option<String> {
    if path.contains('\\') || path.starts_with('/') {
        return None;
    }
    let mut components = path.split('/');
    if components.any(|part| part.is_empty() || part == "." || part == "..") {
        return None;
    }
    let name = path.rsplit('/').next()?;
    let valid = name.len() <= 200
        && !name.starts_with(['.', '-'])
        && name.to_ascii_lowercase().ends_with(".gguf")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'));
    valid.then(|| name.to_owned())
}

/// Quantization names as they appear in GGUF file names, longest first.
const QUANTS: [&str; 44] = [
    "Q4_K_XL", "Q5_K_XL", "Q6_K_XL", "Q8_K_XL", "Q3_K_XL", "Q2_K_XL", "IQ2_XXS", "IQ3_XXS",
    "IQ1_S", "IQ1_M", "IQ2_XS", "IQ2_S", "IQ2_M", "IQ3_XS", "IQ3_S", "IQ3_M", "IQ4_XS", "IQ4_NL",
    "Q2_K_S", "Q2_K_L", "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q4_K_S", "Q4_K_M", "Q4_K_L", "Q5_K_S",
    "Q5_K_M", "Q5_K_L", "Q6_K_L", "TQ1_0", "TQ2_0", "MXFP4", "Q2_K", "Q3_K", "Q4_0", "Q4_1",
    "Q5_0", "Q5_1", "Q6_K", "Q8_0", "BF16", "F16", "F32",
];

/// The quantization named in a GGUF file name, such as `Q4_K_M`.
pub fn quant_of(file_name: &str) -> Option<String> {
    let upper = file_name.to_ascii_uppercase();
    let upper = upper.strip_suffix(".GGUF").unwrap_or(&upper);
    let bytes = upper.as_bytes();
    let boundary = |index: Option<usize>| {
        index.is_none_or(|index| matches!(bytes.get(index), None | Some(b'-' | b'.' | b'_')))
    };
    QUANTS
        .iter()
        .filter(|quant| {
            upper.match_indices(*quant).any(|(start, _)| {
                boundary(start.checked_sub(1)) && boundary(Some(start + quant.len()))
            })
        })
        .max_by_key(|quant| quant.len())
        .map(|quant| (*quant).to_owned())
}

/// `(stem, part, of)` for a split file such as `Model-Q4_K_M-00002-of-00003.gguf`.
fn split_part(name: &str) -> Option<(&str, u32, u32)> {
    let stem = name.get(..name.len().checked_sub(5)?)?;
    let (rest, total) = stem.rsplit_once("-of-")?;
    let (base, part) = rest.rsplit_once('-')?;
    if part.len() != 5 || total.len() != 5 {
        return None;
    }
    let part = part.parse().ok()?;
    let total = total.parse().ok()?;
    (part >= 1 && part <= total && total <= 256).then_some((base, part, total))
}

/// A split set: its folder and stem, and how many parts it has.
type SplitKey = (String, u32);

/// Read a repository answer into its downloadable variants.
pub fn parse_repo(payload: &[u8]) -> Result<Repo, UpdateError> {
    let wire: WireRepo = serde_json::from_slice(payload).map_err(|_| {
        UpdateError("The model information from Hugging Face could not be read.".into())
    })?;
    if !valid_repo_id(&wire.id) || !valid_commit(&wire.sha) {
        return fail("Hugging Face returned a repository name or commit Argentum does not accept.");
    }
    let license = wire
        .card_data
        .as_ref()
        .and_then(|card| card.license.as_ref())
        .and_then(|license| match license {
            serde_json::Value::String(text) => Some(clean_text(text)),
            serde_json::Value::Array(items) => {
                items.first().and_then(|item| item.as_str()).map(clean_text)
            }
            _ => None,
        })
        .or_else(|| license_from_tags(&wire.tags));

    let mut skipped_files = Vec::new();
    let mut singles = Vec::new();
    // (folder and stem, total parts) -> parts found
    let mut splits: Vec<(SplitKey, Vec<(u32, RepoFile)>)> = Vec::new();
    for sibling in wire.siblings {
        if !sibling.rfilename.to_ascii_lowercase().ends_with(".gguf") {
            continue;
        }
        let checked = sibling.lfs.as_ref().filter(|lfs| {
            valid_sha256(&lfs.sha256) && sibling.size.is_none_or(|size| size == lfs.size)
        });
        let (Some(lfs), Some(name)) = (checked, local_name(&sibling.rfilename)) else {
            skipped_files.push(clean_text(&sibling.rfilename));
            continue;
        };
        let file = RepoFile {
            path: sibling.rfilename.clone(),
            local_name: name.clone(),
            size_bytes: lfs.size,
            sha256: lfs.sha256.to_ascii_lowercase(),
        };
        match split_part(&name) {
            Some((stem, part, total)) => {
                let folder = sibling
                    .rfilename
                    .rsplit_once('/')
                    .map_or("", |(folder, _)| folder);
                let key = (format!("{folder}/{stem}"), total);
                match splits.iter_mut().find(|(existing, _)| *existing == key) {
                    Some((_, parts)) => parts.push((part, file)),
                    None => splits.push((key, vec![(part, file)])),
                }
            }
            None => singles.push(file),
        }
    }

    let kind_of = |name: &str| {
        if name.to_ascii_lowercase().contains("mmproj") {
            VariantKind::Projector
        } else {
            VariantKind::Model
        }
    };
    let mut variants: Vec<Variant> = singles
        .into_iter()
        .map(|file| Variant {
            quant: quant_of(&file.local_name),
            kind: kind_of(&file.local_name),
            files: vec![file],
        })
        .collect();
    for ((_, total), mut parts) in splits {
        parts.sort_by_key(|(part, _)| *part);
        let complete = parts.len() == total as usize
            && parts
                .iter()
                .enumerate()
                .all(|(index, (part, _))| *part as usize == index + 1);
        if !complete {
            skipped_files.extend(parts.into_iter().map(|(_, file)| file.path));
            continue;
        }
        let files: Vec<RepoFile> = parts.into_iter().map(|(_, file)| file).collect();
        variants.push(Variant {
            quant: quant_of(&files[0].local_name),
            kind: kind_of(&files[0].local_name),
            files,
        });
    }
    // Two files with the same local name in different folders cannot both be placed.
    let mut seen = std::collections::HashSet::new();
    variants.retain(|variant| {
        let unique = variant
            .files
            .iter()
            .all(|file| seen.insert(file.local_name.clone()));
        if !unique {
            skipped_files.extend(variant.files.iter().map(|file| file.path.clone()));
        }
        unique
    });
    variants.sort_by(|left, right| {
        (left.kind == VariantKind::Projector, left.size_bytes())
            .cmp(&(right.kind == VariantKind::Projector, right.size_bytes()))
    });

    Ok(Repo {
        id: wire.id,
        commit: wire.sha.to_ascii_lowercase(),
        license,
        gated: is_gated(&wire.gated),
        context_length: wire.gguf.as_ref().and_then(|gguf| gguf.context_length),
        architecture: wire
            .gguf
            .as_ref()
            .and_then(|gguf| gguf.architecture.as_deref())
            .map(clean_text),
        parameters: wire.gguf.as_ref().and_then(|gguf| gguf.total),
        variants,
        skipped_files,
    })
}

/// Read a search answer.
pub fn parse_search(payload: &[u8]) -> Result<Vec<RepoSummary>, UpdateError> {
    let wire: Vec<WireSummary> = serde_json::from_slice(payload).map_err(|_| {
        UpdateError("The search results from Hugging Face could not be read.".into())
    })?;
    Ok(wire
        .into_iter()
        .filter(|summary| valid_repo_id(&summary.id))
        .map(|summary| RepoSummary {
            license: license_from_tags(&summary.tags),
            gated: is_gated(&summary.gated),
            id: summary.id,
            downloads: summary.downloads,
            likes: summary.likes,
        })
        .collect())
}

/// What a download of one variant needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    /// Files already in the model folder at the expected size. Their checksum
    /// is confirmed when the download runs.
    pub present: Vec<String>,
    /// Bytes still to transfer, after any partial downloads.
    pub remaining_bytes: u64,
    /// Bytes already on disk in partial downloads that will be resumed.
    pub resumed_bytes: u64,
    /// Free space on the model folder's drive, when it can be read.
    pub free_bytes: Option<u64>,
}

impl DownloadPlan {
    /// The disk space check: the remaining bytes plus [`DISK_RESERVE_BYTES`]
    /// must be free. `None` when the space could not be read.
    pub fn space_problem(&self) -> Option<String> {
        let free = self.free_bytes?;
        let needed = self.remaining_bytes.saturating_add(DISK_RESERVE_BYTES);
        (free < needed).then(|| {
            format!(
                "Not enough disk space: the download needs {} plus {} kept free, and the drive has {} free.",
                size_text(self.remaining_bytes),
                size_text(DISK_RESERVE_BYTES),
                size_text(free)
            )
        })
    }
}

/// Sizes in decimal units, as Settings shows them.
pub fn size_text(bytes: u64) -> String {
    let megabytes = bytes as f64 / 1_000_000.0;
    if megabytes >= 1000.0 {
        format!("{:.2} GB", megabytes / 1000.0)
    } else {
        format!("{megabytes:.1} MB")
    }
}

/// What is left to download for `variant` into `models_dir`. Reads only file
/// sizes and the partial-download records, so it is quick.
pub fn plan_download(
    repo: &Repo,
    variant: &Variant,
    models_dir: &Path,
) -> Result<DownloadPlan, UpdateError> {
    let mut plan = DownloadPlan {
        present: Vec::new(),
        remaining_bytes: 0,
        resumed_bytes: 0,
        free_bytes: free_space(existing_ancestor(models_dir)),
    };
    for file in &variant.files {
        let target = models_dir.join(&file.local_name);
        match fs::symlink_metadata(&target) {
            Ok(meta) if meta.is_file() && meta.len() == file.size_bytes => {
                plan.present.push(file.local_name.clone());
                continue;
            }
            Ok(_) => {
                return fail(format!(
                    "{} already exists in the model folder and is not this file. Move it away and try again.",
                    file.local_name
                ))
            }
            Err(_) => {}
        }
        let resumable = resumable_length(repo, file, &target);
        plan.resumed_bytes = plan.resumed_bytes.saturating_add(resumable);
        plan.remaining_bytes = plan
            .remaining_bytes
            .saturating_add(file.size_bytes.saturating_sub(resumable));
    }
    Ok(plan)
}

fn existing_ancestor(path: &Path) -> &Path {
    path.ancestors()
        .find(|ancestor| ancestor.exists())
        .unwrap_or(path)
}

/// Free bytes on the drive that holds `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0_u64;
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call,
        // and `available` is a valid u64 to write. The other outputs are optional.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(available)
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stats = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
        // SAFETY: `path` is NUL-terminated and `stats` is writable storage of
        // the right type, read only after the call reports success.
        let ok = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } == 0;
        if !ok {
            return None;
        }
        // SAFETY: statvfs succeeded, so it filled `stats`.
        let stats = unsafe { stats.assume_init() };
        #[allow(clippy::useless_conversion)]
        Some(u64::from(stats.f_bavail).saturating_mul(u64::from(stats.f_frsize)))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        None
    }
}

/// What a partial download is, so a resume never appends to a different file.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PartialRecord {
    repo: String,
    commit: String,
    path: String,
    size_bytes: u64,
    sha256: String,
}

impl PartialRecord {
    fn for_file(repo: &Repo, file: &RepoFile) -> Self {
        Self {
            repo: repo.id.clone(),
            commit: repo.commit.clone(),
            path: file.path.clone(),
            size_bytes: file.size_bytes,
            sha256: file.sha256.clone(),
        }
    }
}

fn partial_paths(target: &Path) -> (PathBuf, PathBuf) {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    (
        target.with_file_name(format!("{name}{PARTIAL_SUFFIX}")),
        target.with_file_name(format!("{name}{PARTIAL_RECORD_SUFFIX}")),
    )
}

/// Bytes of a matching partial download for `file`, or 0.
fn resumable_length(repo: &Repo, file: &RepoFile, target: &Path) -> u64 {
    let (partial, record) = partial_paths(target);
    let matches = fs::read(&record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PartialRecord>(&bytes).ok())
        .is_some_and(|record| record == PartialRecord::for_file(repo, file));
    if !matches {
        return 0;
    }
    fs::symlink_metadata(&partial)
        .ok()
        .filter(|meta| meta.is_file() && meta.len() <= file.size_bytes)
        .map_or(0, |meta| meta.len())
}

/// Delete the partial download of every file in `variant`, for "Discard".
pub fn discard_partial(variant: &Variant, models_dir: &Path) {
    for file in &variant.files {
        let (partial, record) = partial_paths(&models_dir.join(&file.local_name));
        let _ = fs::remove_file(partial);
        let _ = fs::remove_file(record);
    }
}

/// Progress of one variant download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Zero-based index of the file being worked on.
    pub file: usize,
    pub files: usize,
    pub received: u64,
    pub total: u64,
    /// True while an existing file or partial download is being hashed.
    pub checking: bool,
}

/// The Hub API client.
pub struct Hub {
    client: reqwest::Client,
    base: Url,
    token: Option<HubToken>,
}

impl Hub {
    /// The public Hub over https. `token` is only needed for gated models.
    pub fn new(token: Option<HubToken>) -> Result<Self, UpdateError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .user_agent(concat!("Argentum/", env!("CARGO_PKG_VERSION")))
            .https_only(true)
            .build()
            .map_err(|_| UpdateError("The Hugging Face request could not be prepared.".into()))?;
        Ok(Self {
            client,
            base: Url::parse(HUB_URL).map_err(|_| UpdateError("Bad Hub address.".into()))?,
            token,
        })
    }

    #[cfg(test)]
    fn local(base: &str, token: Option<HubToken>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .read_timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
            base: Url::parse(base).expect("base"),
            token,
        }
    }

    fn request(&self, url: Url) -> reqwest::RequestBuilder {
        let same_host = url.host_str() == self.base.host_str()
            && url.port_or_known_default() == self.base.port_or_known_default();
        let request = self.client.get(url);
        match (&self.token, same_host) {
            (Some(token), true) => request.bearer_auth(&token.0),
            _ => request,
        }
    }

    fn status_error(status: reqwest::StatusCode, gated_hint: bool) -> UpdateError {
        UpdateError(match status.as_u16() {
            401 | 403 if gated_hint => "This model is gated. Accept its terms on huggingface.co, then set a Hugging Face token in Argentum and try again.".into(),
            401 => "Hugging Face did not accept the token. Check it, or remove it to browse public models.".into(),
            403 => "Hugging Face refused access to this model. It may be gated: accept its terms on huggingface.co and set a token.".into(),
            404 => "Hugging Face has no such model or file.".into(),
            429 => "Hugging Face is limiting requests. Wait a minute and try again.".into(),
            code => format!("Hugging Face answered with status {code}."),
        })
    }

    async fn get_json(&self, url: Url) -> Result<Vec<u8>, UpdateError> {
        let mut response = self
            .request(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|_| UpdateError("Argentum could not reach Hugging Face.".into()))?;
        if !response.status().is_success() {
            return Err(Self::status_error(response.status(), false));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            UpdateError(format!(
                "The answer from Hugging Face stopped early: {error}"
            ))
        })? {
            if body.len() + chunk.len() > MAX_METADATA_BYTES {
                return fail("The answer from Hugging Face was larger than expected.");
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    /// GGUF repositories matching `query`, most downloaded first.
    pub async fn search(&self, query: &str) -> Result<Vec<RepoSummary>, UpdateError> {
        let query: String = query.trim().chars().take(100).collect();
        let mut url = self
            .base
            .join("api/models")
            .map_err(|_| UpdateError("Bad Hub address.".into()))?;
        url.query_pairs_mut()
            .append_pair("search", &query)
            .append_pair("filter", "gguf")
            .append_pair("sort", "downloads")
            .append_pair("direction", "-1")
            .append_pair("limit", &SEARCH_LIMIT.to_string());
        parse_search(&self.get_json(url).await?)
    }

    /// One repository with its GGUF files, their sizes and published SHA-256.
    pub async fn repo(&self, id: &str) -> Result<Repo, UpdateError> {
        if !valid_repo_id(id) {
            return fail("That is not a Hugging Face model name. Use the form owner/name.");
        }
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| UpdateError("Bad Hub address.".into()))?
            .pop_if_empty()
            .extend(["api", "models"])
            .extend(id.split('/'));
        url.query_pairs_mut().append_pair("blobs", "true");
        let repo = parse_repo(&self.get_json(url).await?)?;
        if !repo.id.eq_ignore_ascii_case(id) {
            return fail("Hugging Face answered for a different model.");
        }
        Ok(repo)
    }

    fn file_url(&self, repo: &Repo, file: &RepoFile) -> Result<Url, UpdateError> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| UpdateError("Bad Hub address.".into()))?
            .pop_if_empty()
            .extend(repo.id.split('/'))
            .extend(["resolve", repo.commit.as_str()])
            .extend(file.path.split('/'));
        Ok(url)
    }

    /// Download every file of `variant` into `models_dir` and return the path
    /// of the file the runtime opens. Files already present are kept once their
    /// checksum matches. Dropping the future leaves partial files to resume.
    pub async fn download(
        &self,
        repo: &Repo,
        variant: &Variant,
        models_dir: &Path,
        mut progress: impl FnMut(Progress),
    ) -> Result<PathBuf, UpdateError> {
        if !repo.variants.contains(variant) {
            return fail("That file is not in the listed repository.");
        }
        let plan = plan_download(repo, variant, models_dir)?;
        if let Some(problem) = plan.space_problem() {
            return Err(UpdateError(problem));
        }
        fs::create_dir_all(models_dir).map_err(|error| {
            UpdateError(format!("The model folder could not be created: {error}"))
        })?;
        let files = variant.files.len();
        for (index, file) in variant.files.iter().enumerate() {
            let target = models_dir.join(&file.local_name);
            let mut report = |received, checking| {
                progress(Progress {
                    file: index,
                    files,
                    received,
                    total: file.size_bytes,
                    checking,
                })
            };
            if fs::symlink_metadata(&target).is_ok() {
                report(0, true);
                if file_sha256(&target).as_deref() == Some(file.sha256.as_str()) {
                    continue;
                }
                return fail(format!(
                    "{} is already in the model folder but does not match the published checksum. Move it away and try again.",
                    file.local_name
                ));
            }
            self.download_file(repo, file, &target, &mut report).await?;
        }
        Ok(models_dir.join(variant.model_file()))
    }

    async fn download_file(
        &self,
        repo: &Repo,
        file: &RepoFile,
        target: &Path,
        report: &mut impl FnMut(u64, bool),
    ) -> Result<(), UpdateError> {
        let (partial, record_path) = partial_paths(target);
        let record = PartialRecord::for_file(repo, file);
        let mut offset = resumable_length(repo, file, target);
        let mut hasher = Sha256::new();
        if offset > 0 {
            report(offset, true);
            match hash_prefix(&partial, offset, &mut hasher) {
                Ok(()) => {}
                Err(_) => {
                    offset = 0;
                    hasher = Sha256::new();
                }
            }
        }
        if offset == 0 {
            let _ = fs::remove_file(&partial);
            let bytes = serde_json::to_vec(&record)
                .map_err(|_| UpdateError("The download record could not be written.".into()))?;
            fs::write(&record_path, bytes).map_err(|error| {
                UpdateError(format!("The download record could not be written: {error}"))
            })?;
        }

        let mut response = None;
        if offset < file.size_bytes {
            let mut request = self.request(self.file_url(repo, file)?);
            if offset > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            let answer = request
                .send()
                .await
                .map_err(|error| UpdateError(format!("The download could not start: {error}")))?;
            match answer.status().as_u16() {
                206 if offset > 0 && content_range_start(&answer) == Some(offset) => {}
                200 => {
                    // The server sent the whole file, so start over.
                    offset = 0;
                    hasher = Sha256::new();
                }
                _ if !answer.status().is_success() => {
                    return Err(Self::status_error(answer.status(), repo.gated));
                }
                _ => return fail(
                    "Hugging Face answered the resumed download in a way Argentum does not accept.",
                ),
            }
            if let Some(length) = answer.content_length() {
                if offset.saturating_add(length) != file.size_bytes {
                    return fail("The download is not the expected size, so it was stopped.");
                }
            }
            response = Some(answer);
        }

        let mut output = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&partial)
            .map_err(|error| {
                UpdateError(format!("The download file could not be opened: {error}"))
            })?;
        output.set_len(offset).map_err(|error| {
            UpdateError(format!("The download file could not be prepared: {error}"))
        })?;
        use std::io::Seek;
        output
            .seek(std::io::SeekFrom::Start(offset))
            .map_err(|error| {
                UpdateError(format!("The download file could not be prepared: {error}"))
            })?;

        let mut received = offset;
        if let Some(mut response) = response {
            while let Some(chunk) = response.chunk().await.map_err(|error| {
                UpdateError(format!(
                    "The download stopped early: {error}. Download again to resume."
                ))
            })? {
                received = received.saturating_add(chunk.len() as u64);
                if received > file.size_bytes {
                    drop(output);
                    let _ = fs::remove_file(&partial);
                    let _ = fs::remove_file(&record_path);
                    return fail(
                        "The download is larger than the published file, so it was discarded.",
                    );
                }
                hasher.update(&chunk);
                output.write_all(&chunk).map_err(|error| {
                    UpdateError(format!(
                        "The download could not be saved ({error}). Check free disk space."
                    ))
                })?;
                report(received, false);
            }
        }
        if received != file.size_bytes {
            return fail("The download stopped early. Download again to resume.");
        }
        output
            .sync_all()
            .map_err(|error| UpdateError(format!("The download could not be saved: {error}")))?;
        drop(output);
        if crate::hex(&hasher.finalize()) != file.sha256 {
            let _ = fs::remove_file(&partial);
            let _ = fs::remove_file(&record_path);
            return fail(format!(
                "{} does not match the SHA-256 Hugging Face publishes for it, so it was deleted.",
                file.local_name
            ));
        }
        if fs::symlink_metadata(target).is_ok() {
            return fail(format!(
                "{} appeared in the model folder during the download. Move it away and try again.",
                file.local_name
            ));
        }
        fs::rename(&partial, target).map_err(|error| {
            UpdateError(format!("The model could not be put in place: {error}"))
        })?;
        let _ = fs::remove_file(&record_path);
        Ok(())
    }
}

fn content_range_start(response: &reqwest::Response) -> Option<u64> {
    let value = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    let range = value.strip_prefix("bytes ")?;
    range.split('-').next()?.parse().ok()
}

fn hash_prefix(path: &Path, length: u64, hasher: &mut Sha256) -> std::io::Result<()> {
    let mut file = fs::File::open(path)?;
    let mut remaining = length;
    let mut buffer = vec![0_u8; 1024 * 1024];
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..want])?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(())
}

fn file_sha256(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(crate::hex(&hasher.finalize()))
}

#[cfg(test)]
mod tests;
