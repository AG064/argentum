// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The optional local model add-on: the llama.cpp runtime and one model, which
//! the owner installs from Settings instead of receiving them with the app.
//!
//! Every file is pinned here by URL, size, and SHA-256. The runtime archive must
//! also be listed, with that SHA-256, in llama.cpp's own signed build attestation
//! (see [`crate::sigstore`]) before any byte is downloaded. Model files have no
//! publisher signature to check, so they rest on their pinned SHA-256 alone.
//!
//! Installing is a transaction:
//!
//! 1. Leftovers from an earlier attempt are rolled back.
//! 2. Files download into a staging folder and are checked there.
//! 3. A journal lists the files about to be placed, then they are moved into the
//!    paths the local runtime already reads.
//! 4. The journal is deleted. That is the commit.
//!
//! A failure or cancellation before step 4 removes what this attempt added, and
//! [`recover`] does the same at the next start if the process was killed. Nothing
//! here starts a process. The runtime is only launched later, through the
//! existing local runtime approval.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sigstore::{verify_attestation, PublisherIdentity};
use crate::{PartialFile, UpdateError};

/// One file to download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Artifact {
    pub file_name: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePin {
    pub label: &'static str,
    pub artifact: Artifact,
    pub license: &'static str,
    /// The publisher's Sigstore bundle for `artifact`, pinned with it.
    pub attestation: &'static [u8],
    pub publisher: PublisherIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPin {
    pub id: &'static str,
    pub label: &'static str,
    pub artifact: Artifact,
    pub license: &'static str,
    pub source: &'static str,
}

/// llama.cpp b10621 for Windows x64 with the Vulkan backend. These values must
/// match the local runtime's own pin in `argentum-providers`, which re-checks the
/// archive and its `llama-server.exe` before every launch.
pub const LLAMA_CPP_RUNTIME: RuntimePin = RuntimePin {
    label: "llama.cpp b10621, Windows x64, Vulkan",
    artifact: Artifact {
        file_name: "llama-b10621-bin-win-vulkan-x64.zip",
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b10621/llama-b10621-bin-win-vulkan-x64.zip",
        size_bytes: 34_403_304,
        sha256: "2672d85bf87c8280d94dee01eb6a86280046878f70a07d786a93637fa9081163",
    },
    license: "MIT",
    attestation: include_bytes!("../pins/llama-b10621-bin-win-vulkan-x64.sigstore.json"),
    publisher: PublisherIdentity {
        workflow:
            "https://github.com/ggml-org/llama.cpp/.github/workflows/release.yml@refs/heads/master",
        issuer: "https://token.actions.githubusercontent.com",
    },
};

/// The models the owner can pick from, each pinned to one repository commit.
pub const MODELS: [ModelPin; 3] = [
    ModelPin {
        id: "Qwen3.5-0.8B-Q8_0.gguf",
        label: "Qwen3.5 0.8B Q8_0",
        artifact: Artifact {
            file_name: "Qwen3.5-0.8B-Q8_0.gguf",
            url: "https://huggingface.co/ggml-org/Qwen3.5-0.8B-GGUF/resolve/8fea620810c4afa23dd6443f999a48574c1611a3/Qwen3.5-0.8B-Q8_0.gguf",
            size_bytes: 833_592_096,
            sha256: "37ae482d336108d23516fa35e8e0c4126688d81018b87178a18d752a1357814f",
        },
        license: "Apache-2.0",
        source: "Hugging Face, ggml-org/Qwen3.5-0.8B-GGUF",
    },
    ModelPin {
        id: "Qwen3.5-2B-Q4_K_M.gguf",
        label: "Qwen3.5 2B Q4_K_M",
        artifact: Artifact {
            file_name: "Qwen3.5-2B-Q4_K_M.gguf",
            url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/f6d5376be1edb4d416d56da11e5397a961aca8ae/Qwen3.5-2B-Q4_K_M.gguf",
            size_bytes: 1_280_835_840,
            sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        },
        license: "Apache-2.0",
        source: "Hugging Face, unsloth/Qwen3.5-2B-GGUF",
    },
    ModelPin {
        id: "Qwen3.5-4B-Q4_K_M.gguf",
        label: "Qwen3.5 4B Q4_K_M",
        artifact: Artifact {
            file_name: "Qwen3.5-4B-Q4_K_M.gguf",
            url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/e87f176479d0855a907a41277aca2f8ee7a09523/Qwen3.5-4B-Q4_K_M.gguf",
            size_bytes: 2_740_937_888,
            sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        },
        license: "Apache-2.0",
        source: "Hugging Face, unsloth/Qwen3.5-4B-GGUF",
    },
];

pub fn model(id: &str) -> Option<&'static ModelPin> {
    MODELS.iter().find(|model| model.id == id)
}

/// Where the add-on lives. `runtime_bundle` and `models_dir` are the paths the
/// local runtime already reads, so placing files there is what registers them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The runtime archive, `.../runtimes/llama-cpp/<version>/<backend>/bundle.zip`.
    pub runtime_bundle: PathBuf,
    /// The folder removal deletes, which also holds extracted runtime sessions.
    pub runtime_dir: PathBuf,
    pub models_dir: PathBuf,
    /// Staging and the install journal. Must be on the same drive as the others.
    pub work_dir: PathBuf,
}

impl Layout {
    fn staging(&self) -> PathBuf {
        self.work_dir.join("staging")
    }

    fn journal(&self) -> PathBuf {
        self.work_dir.join("install-journal.json")
    }

    fn model_path(&self, model: &ModelPin) -> PathBuf {
        self.models_dir.join(model.artifact.file_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub runtime_installed: bool,
    pub models: Vec<&'static ModelPin>,
}

impl Status {
    pub fn installed(&self) -> bool {
        self.runtime_installed && !self.models.is_empty()
    }
}

fn has_size(path: &Path, size: u64) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == size)
}

/// What is installed, judged by file sizes. The runtime checks each file's full
/// SHA-256 again before it launches anything.
pub fn status(layout: &Layout) -> Status {
    Status {
        runtime_installed: has_size(
            &layout.runtime_bundle,
            LLAMA_CPP_RUNTIME.artifact.size_bytes,
        ),
        models: MODELS
            .iter()
            .filter(|model| has_size(&layout.model_path(model), model.artifact.size_bytes))
            .collect(),
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Journal {
    placed: Vec<PathBuf>,
}

/// Roll back an install that never committed: delete the staging folder and any
/// file the journal says was being placed. Call at startup and before installing.
pub fn recover(layout: &Layout) -> Result<(), UpdateError> {
    let journal = layout.journal();
    if let Ok(bytes) = fs::read(&journal) {
        let Ok(entries) = serde_json::from_slice::<Journal>(&bytes) else {
            return Err(UpdateError(
                "An earlier add-on install left a damaged record. Remove the add-on to clean up."
                    .into(),
            ));
        };
        for path in entries.placed {
            if is_inside(&path, layout) {
                remove_file_if_present(&path)?;
            }
        }
        remove_file_if_present(&journal)?;
    }
    match fs::remove_dir_all(layout.staging()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(UpdateError(
            "Files from an earlier add-on download could not be removed.".into(),
        )),
    }
}

fn is_inside(path: &Path, layout: &Layout) -> bool {
    *path == layout.runtime_bundle || MODELS.iter().any(|model| *path == layout.model_path(model))
}

fn remove_file_if_present(path: &Path) -> Result<(), UpdateError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(UpdateError(format!(
            "{} could not be removed. Close anything using it and try again.",
            path.display()
        ))),
    }
}

/// Which step an install is on, for the progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    CheckingSignature,
    Downloading(&'static str),
    Placing,
}

/// The pieces an install fetches from outside. Production uses [`Network`].
#[allow(async_fn_in_trait)]
pub trait Source {
    async fn fetch(
        &self,
        artifact: &Artifact,
        destination: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError>;
}

/// Downloads each artifact from its pinned https URL.
pub struct Network {
    client: reqwest::Client,
}

impl Network {
    pub fn new() -> Result<Self, UpdateError> {
        Ok(Self {
            client: crate::client()?,
        })
    }
}

impl Source for Network {
    async fn fetch(
        &self,
        artifact: &Artifact,
        destination: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), UpdateError> {
        download_artifact(&self.client, artifact.url, artifact, destination, progress).await
    }
}

/// Download `url` to `destination`, keeping it only if it matches `artifact`'s
/// pinned size and SHA-256.
pub(crate) async fn download_artifact(
    client: &reqwest::Client,
    url: &str,
    artifact: &Artifact,
    destination: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), UpdateError> {
    let partial = destination.with_extension("partial");
    let guard = PartialFile::create(&partial)
        .map_err(|_| UpdateError("The download file could not be created.".into()))?;
    let mut progress = progress;
    crate::stream_checked(
        client,
        url,
        artifact.sha256,
        Some(artifact.size_bytes),
        artifact.size_bytes,
        guard.file()?,
        &mut progress,
    )
    .await?;
    guard.finish(destination)
}

/// Rolls back whatever an unfinished install added, including when the install
/// future is dropped part way, for example by a Cancel button.
struct Transaction<'a> {
    layout: &'a Layout,
    placed: Vec<PathBuf>,
    committed: bool,
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for path in &self.placed {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_file(self.layout.journal());
        // A download still open at this point is removed by its own guard, and
        // Windows keeps an open file's folder, so clear staging file by file.
        // `recover` removes the empty folder later.
        if let Ok(entries) = fs::read_dir(self.layout.staging()) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        let _ = fs::remove_dir_all(self.layout.staging());
    }
}

fn file_matches(path: &Path, artifact: &Artifact) -> bool {
    if !has_size(path, artifact.size_bytes) {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).is_ok()
        && crate::hex(&hasher.finalize()) == artifact.sha256
}

/// Install the runtime and `model`. Files already present with the right
/// checksum are kept. `check_runtime` runs on the downloaded runtime archive
/// before it is placed, so the caller can apply the local runtime's own checks.
pub async fn install(
    layout: &Layout,
    model: &ModelPin,
    source: &impl Source,
    check_runtime: impl Fn(&Path) -> Result<(), UpdateError>,
    mut progress: impl FnMut(Step, u64, Option<u64>),
) -> Result<(), UpdateError> {
    if !MODELS.contains(model) {
        return Err(UpdateError("That model is not on the list.".into()));
    }
    recover(layout)?;

    progress(Step::CheckingSignature, 0, None);
    let runtime = LLAMA_CPP_RUNTIME;
    verify_attestation(
        runtime.attestation,
        runtime.publisher,
        runtime.artifact.file_name,
        runtime.artifact.sha256,
    )?;

    let staging = layout.staging();
    fs::create_dir_all(&staging)
        .map_err(|_| UpdateError("The add-on download folder could not be created.".into()))?;
    let mut transaction = Transaction {
        layout,
        placed: Vec::new(),
        committed: false,
    };

    let mut moves = Vec::new();
    if !file_matches(&layout.runtime_bundle, &runtime.artifact) {
        let staged = staging.join(runtime.artifact.file_name);
        source
            .fetch(&runtime.artifact, &staged, &mut |received, total| {
                progress(Step::Downloading(runtime.label), received, total)
            })
            .await?;
        check_runtime(&staged)?;
        moves.push((staged, layout.runtime_bundle.clone()));
    }
    let model_target = layout.model_path(model);
    if !file_matches(&model_target, &model.artifact) {
        if fs::symlink_metadata(&model_target).is_ok() {
            return Err(UpdateError(format!(
                "{} already exists and is not the expected file. Move it away and try again.",
                model_target.display()
            )));
        }
        let staged = staging.join(model.artifact.file_name);
        source
            .fetch(&model.artifact, &staged, &mut |received, total| {
                progress(Step::Downloading(model.label), received, total)
            })
            .await?;
        moves.push((staged, model_target));
    }

    progress(Step::Placing, 0, None);
    let journal = Journal {
        placed: moves.iter().map(|(_, target)| target.clone()).collect(),
    };
    let journal_bytes = serde_json::to_vec(&journal)
        .map_err(|_| UpdateError("The install record could not be written.".into()))?;
    fs::write(layout.journal(), journal_bytes)
        .map_err(|_| UpdateError("The install record could not be written.".into()))?;
    for (staged, target) in moves {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| UpdateError("The add-on folder could not be created.".into()))?;
        }
        if fs::symlink_metadata(&target).is_ok() {
            // Present but wrong, such as a damaged runtime archive. Replace it.
            remove_file_if_present(&target)?;
        }
        fs::rename(&staged, &target)
            .map_err(|_| UpdateError("The add-on could not be put in place.".into()))?;
        transaction.placed.push(target);
    }
    fs::remove_file(layout.journal())
        .map_err(|_| UpdateError("The install record could not be closed.".into()))?;
    transaction.committed = true;
    let _ = fs::remove_dir_all(staging);
    Ok(())
}

/// Remove the runtime and every model from [`MODELS`]. Other model files in the
/// folder are the owner's and stay. The caller stops the runtime first.
pub fn remove(layout: &Layout) -> Result<(), UpdateError> {
    recover(layout)?;
    // The archive goes first: without it the runtime cannot launch, so a later
    // failure still leaves the add-on reported, and treated, as not installed.
    remove_file_if_present(&layout.runtime_bundle)?;
    match fs::remove_dir_all(&layout.runtime_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(UpdateError(
            "The runtime folder could not be fully removed. Stop the local model and try again."
                .into(),
        )),
    }
    for model in &MODELS {
        remove_file_if_present(&layout.model_path(model))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::{Read, Write};

    /// Serves fixed files, or stops part way through one, over a local socket.
    enum Reply {
        Whole(Vec<u8>),
        /// Announces the full length but closes after this many bytes.
        CutAfter(Vec<u8>, usize),
        /// Sends this many bytes, then holds the connection open.
        HangAfter(Vec<u8>, usize),
    }

    fn serve(routes: Vec<(String, Reply)>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0u8; 2048];
                let read = stream.read(&mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..read]).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                match routes.iter().find(|(route, _)| *route == path) {
                    Some((_, Reply::Whole(body))) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(body);
                    }
                    Some((_, Reply::CutAfter(body, cut))) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(&body[..*cut]);
                    }
                    Some((_, Reply::HangAfter(body, cut))) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK
Content-Length: {}
Connection: close

",
                            body.len()
                        );
                        let _ = stream.write_all(&body[..*cut]);
                        let _ = stream.flush();
                        std::thread::sleep(std::time::Duration::from_secs(30));
                    }
                    None => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                }
            }
        });
        format!("http://{address}")
    }

    /// Fetches from a local server instead of the pinned URL, checking each file
    /// against fixture pins with the same code the real download uses.
    struct Fixture {
        base: String,
        pins: Vec<(&'static str, Artifact)>,
        fetched: RefCell<Vec<&'static str>>,
    }

    impl Source for Fixture {
        async fn fetch(
            &self,
            artifact: &Artifact,
            destination: &Path,
            progress: &mut dyn FnMut(u64, Option<u64>),
        ) -> Result<(), UpdateError> {
            self.fetched.borrow_mut().push(artifact.file_name);
            let pin = self
                .pins
                .iter()
                .find(|(name, _)| *name == artifact.file_name)
                .map(|(_, pin)| *pin)
                .expect("fixture pin");
            let url = format!("{}/{}", self.base, artifact.file_name);
            download_artifact(&reqwest::Client::new(), &url, &pin, destination, progress).await
        }
    }

    fn leak(bytes: &[u8]) -> &'static str {
        Box::leak(crate::hex(&Sha256::digest(bytes)).into_boxed_str())
    }

    fn pin_for(artifact: &Artifact, body: &[u8]) -> (&'static str, Artifact) {
        (
            artifact.file_name,
            Artifact {
                size_bytes: body.len() as u64,
                sha256: leak(body),
                ..*artifact
            },
        )
    }

    fn scratch(name: &str) -> Layout {
        let root =
            std::env::temp_dir().join(format!("argentum-addon-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("scratch");
        Layout {
            runtime_bundle: root.join("runtimes/llama-cpp/v0.3.0/vulkan-x64/bundle.zip"),
            runtime_dir: root.join("runtimes/llama-cpp"),
            models_dir: root.join("models"),
            work_dir: root.join("addons"),
        }
    }

    fn root_of(layout: &Layout) -> PathBuf {
        layout.work_dir.parent().expect("root").to_path_buf()
    }

    /// Every file under `root`, relative, sorted.
    fn files(root: &Path) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    out.push(
                        path.strip_prefix(root)
                            .expect("relative")
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    const RUNTIME_BODY: &[u8] = b"pretend runtime archive";
    const MODEL_BODY: &[u8] = b"pretend model weights, a little longer than the runtime";

    fn fixture(runtime: Reply, model: Reply) -> Fixture {
        let model_pin = MODELS[0];
        let base = serve(vec![
            (
                format!("/{}", LLAMA_CPP_RUNTIME.artifact.file_name),
                runtime,
            ),
            (format!("/{}", model_pin.artifact.file_name), model),
        ]);
        Fixture {
            base,
            pins: vec![
                pin_for(&LLAMA_CPP_RUNTIME.artifact, RUNTIME_BODY),
                pin_for(&model_pin.artifact, MODEL_BODY),
            ],
            fetched: RefCell::new(Vec::new()),
        }
    }

    /// The fixture's files are tiny, so status is judged against fixture sizes
    /// here rather than the real pins.
    fn placed(layout: &Layout) -> (bool, bool) {
        (
            fs::read(&layout.runtime_bundle).ok().as_deref() == Some(RUNTIME_BODY),
            fs::read(layout.model_path(&MODELS[0])).ok().as_deref() == Some(MODEL_BODY),
        )
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let layout = scratch("empty");
        let status = status(&layout);
        assert!(!status.runtime_installed);
        assert!(status.models.is_empty());
        assert!(!status.installed());
        recover(&layout).expect("nothing to recover");
        remove(&layout).expect("nothing to remove");
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[test]
    fn status_needs_the_runtime_and_a_listed_model() {
        let layout = scratch("status");
        fs::create_dir_all(&layout.models_dir).expect("models");
        fs::write(layout.models_dir.join("own-model.gguf"), b"x").expect("own model");
        assert!(!status(&layout).installed());
        fs::create_dir_all(layout.runtime_bundle.parent().expect("parent")).expect("dir");
        let runtime = fs::File::create(&layout.runtime_bundle).expect("runtime");
        runtime
            .set_len(LLAMA_CPP_RUNTIME.artifact.size_bytes)
            .expect("size");
        drop(runtime);
        let found = status(&layout);
        assert!(found.runtime_installed);
        assert!(!found.installed(), "an unlisted model does not count");
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn install_places_both_files_and_removal_takes_them_away() {
        let layout = scratch("install");
        let source = fixture(
            Reply::Whole(RUNTIME_BODY.into()),
            Reply::Whole(MODEL_BODY.into()),
        );
        let mut steps = Vec::new();
        install(
            &layout,
            &MODELS[0],
            &source,
            |_| Ok(()),
            |step, _, _| steps.push(step),
        )
        .await
        .expect("install");
        assert_eq!(placed(&layout), (true, true));
        assert_eq!(steps.first(), Some(&Step::CheckingSignature));
        assert_eq!(steps.last(), Some(&Step::Placing));
        // Only the placed files remain. No staging, partial, or journal files.
        assert_eq!(
            files(&root_of(&layout)),
            vec![
                "models/Qwen3.5-0.8B-Q8_0.gguf".to_owned(),
                "runtimes/llama-cpp/v0.3.0/vulkan-x64/bundle.zip".to_owned(),
            ]
        );

        // The owner's own model stays when the add-on is removed.
        fs::write(layout.models_dir.join("own-model.gguf"), b"mine").expect("own model");
        // Any other model from the list is removed too.
        let pinned = layout.model_path(&MODELS[1]);
        fs::write(&pinned, b"listed").expect("pinned");
        remove(&layout).expect("remove");
        assert!(!layout.runtime_bundle.exists());
        assert!(!layout.runtime_dir.exists());
        assert!(!pinned.exists());
        assert_eq!(
            files(&root_of(&layout)),
            vec!["models/own-model.gguf".to_owned()]
        );
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn a_tampered_download_installs_nothing() {
        let layout = scratch("tampered");
        let mut evil = MODEL_BODY.to_vec();
        evil[0] ^= 0x01;
        let source = fixture(Reply::Whole(RUNTIME_BODY.into()), Reply::Whole(evil));
        let error = install(&layout, &MODELS[0], &source, |_| Ok(()), |_, _, _| {})
            .await
            .expect_err("checksum mismatch");
        assert!(error.0.contains("checksum"), "{}", error.0);
        // The runtime downloaded fine, but it is not left behind on its own.
        assert_eq!(placed(&layout), (false, false));
        assert!(
            files(&root_of(&layout)).is_empty(),
            "{:?}",
            files(&root_of(&layout))
        );
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn a_runtime_that_fails_its_own_checks_installs_nothing() {
        let layout = scratch("runtime-check");
        let source = fixture(
            Reply::Whole(RUNTIME_BODY.into()),
            Reply::Whole(MODEL_BODY.into()),
        );
        let error = install(
            &layout,
            &MODELS[0],
            &source,
            |_| {
                Err(UpdateError(
                    "The runtime archive is not the expected layout.".into(),
                ))
            },
            |_, _, _| {},
        )
        .await
        .expect_err("rejected");
        assert!(error.0.contains("expected layout"));
        assert_eq!(
            *source.fetched.borrow(),
            vec![LLAMA_CPP_RUNTIME.artifact.file_name]
        );
        assert!(files(&root_of(&layout)).is_empty());
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn an_interrupted_download_installs_nothing() {
        let layout = scratch("interrupted");
        let source = fixture(
            Reply::Whole(RUNTIME_BODY.into()),
            Reply::CutAfter(MODEL_BODY.into(), MODEL_BODY.len() / 2),
        );
        let error = install(&layout, &MODELS[0], &source, |_| Ok(()), |_, _, _| {})
            .await
            .expect_err("interrupted");
        assert!(error.0.contains("stopped early"), "{}", error.0);
        assert!(
            files(&root_of(&layout)).is_empty(),
            "{:?}",
            files(&root_of(&layout))
        );
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn a_cancelled_install_installs_nothing() {
        let layout = scratch("cancelled");
        let source = fixture(
            Reply::Whole(RUNTIME_BODY.into()),
            Reply::HangAfter(MODEL_BODY.into(), 8),
        );
        // Stop the install as soon as the model download reports progress, the
        // way dropping the task from a Cancel button would.
        let started = std::sync::atomic::AtomicBool::new(false);
        let install = install(
            &layout,
            &MODELS[0],
            &source,
            |_| Ok(()),
            |step, _, _| {
                if step == Step::Downloading(MODELS[0].label) {
                    started.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            },
        );
        let mut install = Box::pin(install);
        loop {
            tokio::select! {
                _ = &mut install => panic!("the install should have been cancelled first"),
                _ = tokio::task::yield_now() => {
                    if started.load(std::sync::atomic::Ordering::SeqCst) { break; }
                }
            }
        }
        drop(install);
        assert!(
            files(&root_of(&layout)).is_empty(),
            "{:?}",
            files(&root_of(&layout))
        );
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn a_killed_install_is_rolled_back_at_the_next_start() {
        let layout = scratch("killed");
        // What a process killed while placing files leaves: a journal, one placed
        // file, and staging leftovers.
        fs::create_dir_all(layout.staging()).expect("staging");
        fs::write(layout.staging().join("model.partial"), b"half").expect("partial");
        fs::create_dir_all(layout.runtime_bundle.parent().expect("parent")).expect("dir");
        fs::write(&layout.runtime_bundle, RUNTIME_BODY).expect("placed");
        let journal = Journal {
            placed: vec![
                layout.runtime_bundle.clone(),
                root_of(&layout).join("elsewhere"),
            ],
        };
        fs::write(
            layout.journal(),
            serde_json::to_vec(&journal).expect("json"),
        )
        .expect("journal");
        fs::write(root_of(&layout).join("elsewhere"), b"not ours").expect("unrelated");

        recover(&layout).expect("recover");
        assert!(!layout.runtime_bundle.exists());
        assert!(!layout.journal().exists());
        assert!(!layout.staging().exists());
        // A journal entry outside the add-on's own paths is never deleted.
        assert!(root_of(&layout).join("elsewhere").exists());
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn files_already_in_place_are_not_downloaded_again() {
        let layout = scratch("present");
        let source = fixture(
            Reply::Whole(RUNTIME_BODY.into()),
            Reply::Whole(MODEL_BODY.into()),
        );
        install(&layout, &MODELS[0], &source, |_| Ok(()), |_, _, _| {})
            .await
            .expect("first install");
        assert_eq!(source.fetched.borrow().len(), 2);
        // With the fixture pins the files do not match the real pins, so a second
        // install would download again. Real pins are covered by `file_matches`.
        assert!(!file_matches(
            &layout.runtime_bundle,
            &LLAMA_CPP_RUNTIME.artifact
        ));
        let fixture_pin = source.pins[0].1;
        assert!(file_matches(&layout.runtime_bundle, &fixture_pin));
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[tokio::test]
    async fn an_unlisted_model_is_refused_before_any_download() {
        let layout = scratch("unlisted");
        let source = fixture(
            Reply::Whole(RUNTIME_BODY.into()),
            Reply::Whole(MODEL_BODY.into()),
        );
        let other = ModelPin {
            id: "other.gguf",
            ..MODELS[0]
        };
        assert!(install(&layout, &other, &source, |_| Ok(()), |_, _, _| {})
            .await
            .is_err());
        assert!(source.fetched.borrow().is_empty());
        fs::remove_dir_all(root_of(&layout)).expect("cleanup");
    }

    #[test]
    fn the_pinned_runtime_attestation_verifies() {
        let runtime = LLAMA_CPP_RUNTIME;
        verify_attestation(
            runtime.attestation,
            runtime.publisher,
            runtime.artifact.file_name,
            runtime.artifact.sha256,
        )
        .expect("the pinned runtime is signed by llama.cpp's release workflow");
    }

    #[test]
    fn pins_are_well_formed() {
        let all = std::iter::once(&LLAMA_CPP_RUNTIME.artifact)
            .chain(MODELS.iter().map(|model| &model.artifact));
        for artifact in all {
            assert_eq!(artifact.sha256.len(), 64);
            assert!(artifact
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(artifact.url.starts_with("https://"));
            assert!(artifact.url.ends_with(artifact.file_name));
        }
        for model in &MODELS {
            assert_eq!(model.id, model.artifact.file_name);
            assert!(model.artifact.file_name.ends_with(".gguf"));
        }
    }
}
