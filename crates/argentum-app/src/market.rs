// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Settings > Providers > Model marketplace: search GGUF models on Hugging
//! Face, see each repository's files with their size, quantization, license
//! and context length, and download one into the local model folder that the
//! llama.cpp runtime already reads.
//!
//! Like the add-on, this belongs to the desktop program. No tool, model or
//! protocol client can reach it. Every request starts from a button: Search,
//! Open, and "Download" after the owner has seen the files, sizes, checks and
//! free disk space. Nothing starts a model. The local runtime still asks for
//! its own launch approval.
//!
//! The optional Hugging Face token is read by the system credential prompt and
//! kept in the OS credential store. It is never shown, logged or put in the
//! interface's text.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use argentum_platform::{AppPaths, HardwareInfo, SecretStore};
use argentum_providers::{launcher_context_tokens, local_model_fit, LocalModelFit, MemoryFacts};
use argentum_security::SecretValue;
use argentum_ui::{App, MainWindow, MarketFileView, MarketRepoView, UiHandle};
use argentum_update::hub::{self, Hub, HubToken, Repo, RepoSummary, Variant, VariantKind};
use argentum_update::UpdateError;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};
use tokio_util::sync::CancellationToken;

/// Where the token is kept in the OS credential store.
pub(crate) const HUB_TOKEN_KEY: &str = "marketplace/huggingface-token";
/// The name the system credential prompt shows.
const HUB_TOKEN_PROMPT: &str = "huggingface-hub";

pub(crate) struct Context {
    pub(crate) secrets: Arc<dyn SecretStore>,
}

#[derive(Default)]
struct Shared {
    repo: Option<Repo>,
    chosen: Option<usize>,
    busy: Option<CancellationToken>,
}

pub(crate) fn connect(ui: &UiHandle, context: Context) {
    let window = ui.window();
    let Some(models_dir) = cfg!(windows)
        .then(AppPaths::discover)
        .and_then(Result::ok)
        .and_then(|paths| crate::local_addon::layout(&paths.data_dir))
        .map(|layout| layout.models_dir)
    else {
        window.global::<App>().set_market_supported(false);
        return;
    };
    let app = window.global::<App>();
    app.set_market_error(false);
    app.set_market_supported(true);
    app.set_market_state("idle".into());
    app.set_market_detail(IDLE_TEXT.into());
    app.set_market_token_set(token(context.secrets.as_ref()).is_some());
    // Read once, locally. Nothing about this computer is sent anywhere.
    let hardware = argentum_platform::detect_hardware();
    app.set_market_hardware(hardware_text(&hardware).into());
    let _ = MEMORY.set(memory_facts(&hardware));

    let models_dir = Arc::new(models_dir);
    let shared = Arc::new(Mutex::new(Shared::default()));
    let secrets = context.secrets;

    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        let secrets = secrets.clone();
        app.on_market_search(move |query| {
            let Some(window) = weak.upgrade() else { return };
            if !take_busy(&shared) {
                return;
            }
            let app = window.global::<App>();
            app.set_market_error(false);
            app.set_market_state("searching".into());
            app.set_market_detail("Searching Hugging Face for GGUF models.".into());
            let token = token(secrets.as_ref());
            let query = query.to_string();
            let weak = weak.clone();
            let shared = shared.clone();
            spawn("market-search", move || {
                let result = block_on(async { Hub::new(token)?.search(&query).await });
                release_busy(&shared);
                let _ = weak.upgrade_in_event_loop(move |window| match result {
                    Ok(results) => show_results(&window, &query, &results),
                    Err(error_value) => show_error(&window, &error_value),
                });
            });
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        let secrets = secrets.clone();
        let models_dir = models_dir.clone();
        app.on_market_open(move |id| {
            let Some(window) = weak.upgrade() else { return };
            if !take_busy(&shared) {
                return;
            }
            let app = window.global::<App>();
            app.set_market_error(false);
            app.set_market_state("loading".into());
            app.set_market_detail(format!("Reading the files of {id}.").into());
            let token = token(secrets.as_ref());
            let has_token = token.is_some();
            let id = id.to_string();
            let weak = weak.clone();
            let shared = shared.clone();
            let models_dir = models_dir.clone();
            spawn("market-open", move || {
                let result = block_on(async { Hub::new(token)?.repo(&id).await });
                release_busy(&shared);
                let _ = weak.upgrade_in_event_loop(move |window| match result {
                    Ok(repo) => {
                        show_repo(&window, &repo, &models_dir, has_token);
                        if let Ok(mut state) = shared.lock() {
                            state.repo = Some(repo);
                            state.chosen = None;
                        }
                    }
                    Err(error_value) => show_error(&window, &error_value),
                });
            });
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        let models_dir = models_dir.clone();
        let secrets = secrets.clone();
        app.on_market_choose(move |index| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            if state.busy.is_some() {
                return;
            }
            let Some(repo) = state.repo.as_ref() else {
                return;
            };
            let Some(variant) = usize::try_from(index)
                .ok()
                .and_then(|index| repo.variants.get(index))
            else {
                return;
            };
            let app = window.global::<App>();
            app.set_market_error(false);
            match hub::plan_download(repo, variant, &models_dir) {
                Ok(plan) => {
                    let problem = plan.space_problem();
                    app.set_market_can_download(problem.is_none());
                    app.set_market_confirm_text(
                        confirm_text(
                            repo,
                            variant,
                            &plan,
                            &models_dir,
                            token(secrets.as_ref()).is_some(),
                        )
                        .into(),
                    );
                    app.set_market_state("confirm".into());
                    state.chosen = usize::try_from(index).ok();
                }
                Err(error_value) => show_error(&window, &error_value),
            }
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        let models_dir = models_dir.clone();
        let secrets = secrets.clone();
        app.on_market_download(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            if state.busy.is_some() {
                return;
            }
            let (Some(repo), Some(index)) = (state.repo.clone(), state.chosen) else {
                return;
            };
            let Some(variant) = repo.variants.get(index).cloned() else { return };
            let cancel = CancellationToken::new();
            state.busy = Some(cancel.clone());
            drop(state);
            let app = window.global::<App>();
    app.set_market_error(false);
            app.set_market_state("downloading".into());
            app.set_market_detail(format!("Starting the download of {}.", variant.label()).into());
            let token = token(secrets.as_ref());
            let weak = weak.clone();
            let shared = shared.clone();
            let models_dir = models_dir.clone();
            spawn("market-download", move || {
                let result = run_download(token, &repo, &variant, &models_dir, &cancel, &weak);
                release_busy(&shared);
                let _ = weak.upgrade_in_event_loop(move |window| {
                    let app = window.global::<App>();
    app.set_market_error(false);
                    match result {
                        Ok(path) => {
                            app.set_market_state("repo".into());
                            app.set_market_detail(
                                format!(
                                    "Downloaded {} to {}. Its SHA-256 matches the one Hugging Face publishes. Choose llama.cpp in Providers and pick this model. Starting it asks for your approval.",
                                    variant.label(),
                                    path.display()
                                )
                                .into(),
                            );
                        }
                        Err(error_value) => show_error(&window, &error_value),
                    }
                });
            });
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        app.on_market_cancel(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(state) = shared.lock() else { return };
            match &state.busy {
                Some(cancel) => cancel.cancel(),
                None => {
                    let app = window.global::<App>();
                    app.set_market_error(false);
                    if state.repo.is_some() {
                        app.set_market_state("repo".into());
                        app.set_market_detail(SharedString::default());
                    } else {
                        app.set_market_state("idle".into());
                        app.set_market_detail(IDLE_TEXT.into());
                    }
                }
            }
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        let models_dir = models_dir.clone();
        app.on_market_discard(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(state) = shared.lock() else { return };
            if state.busy.is_some() {
                return;
            }
            let (Some(repo), Some(index)) = (state.repo.as_ref(), state.chosen) else {
                return;
            };
            if let Some(variant) = repo.variants.get(index) {
                hub::discard_partial(variant, &models_dir);
                let app = window.global::<App>();
                app.set_market_error(false);
                app.set_market_state("repo".into());
                app.set_market_detail(
                    format!("The partial download of {} was deleted.", variant.label()).into(),
                );
            }
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        app.on_market_back(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            if state.busy.is_some() {
                return;
            }
            state.repo = None;
            state.chosen = None;
            let app = window.global::<App>();
            app.set_market_error(false);
            app.set_market_state(if app.get_market_results().row_count() > 0 {
                "results".into()
            } else {
                "idle".into()
            });
            app.set_market_detail(SharedString::default());
        });
    }
    {
        let weak = ui.weak_window();
        let secrets = secrets.clone();
        app.on_market_set_token(move || {
            let Some(window) = weak.upgrade() else { return };
            let app = window.global::<App>();
    app.set_market_error(false);
            // The system prompt reads the token, so it never passes through the
            // interface's text fields.
            match argentum_platform::prompt_provider_credential(HUB_TOKEN_PROMPT) {
                Ok(value) => match HubToken::new(value.expose()) {
                    Some(_) => match secrets.set(HUB_TOKEN_KEY, value) {
                        Ok(()) => {
                            app.set_market_token_set(true);
                            app.set_market_detail("The Hugging Face token is saved in the system credential store. It is sent only to huggingface.co, and only for your searches and downloads.".into());
                        }
                        Err(_) => show_error(
                            &window,
                            &UpdateError("The token could not be saved in the system credential store.".into()),
                        ),
                    },
                    None => show_error(
                        &window,
                        &UpdateError("That does not look like a Hugging Face token.".into()),
                    ),
                },
                Err(argentum_platform::PlatformError::CredentialPromptCancelled) => {}
                Err(_) => show_error(
                    &window,
                    &UpdateError("The credential prompt is not available.".into()),
                ),
            }
        });
    }
    {
        let weak = ui.weak_window();
        app.on_market_clear_token(move || {
            let Some(window) = weak.upgrade() else { return };
            let app = window.global::<App>();
            app.set_market_error(false);
            match secrets.delete(HUB_TOKEN_KEY) {
                Ok(()) => {
                    app.set_market_token_set(false);
                    app.set_market_detail("The Hugging Face token was removed.".into());
                }
                Err(_) => show_error(
                    &window,
                    &UpdateError(
                        "The token could not be removed from the system credential store.".into(),
                    ),
                ),
            }
        });
    }
}

static MEMORY: std::sync::OnceLock<MemoryFacts> = std::sync::OnceLock::new();

const GB: f64 = 1_000_000_000.0;

pub(crate) fn memory_facts(hardware: &HardwareInfo) -> MemoryFacts {
    MemoryFacts {
        system_memory_bytes: hardware.system_memory_bytes,
        gpu_memory_bytes: hardware.main_gpu().map(|gpu| gpu.dedicated_memory_bytes),
    }
}

/// What the marketplace says about this computer. Shown only here.
pub(crate) fn hardware_text(hardware: &HardwareInfo) -> String {
    let ram = if hardware.system_memory_bytes == 0 {
        "system memory could not be read".to_owned()
    } else {
        format!(
            "{:.0} GB of system memory",
            hardware.system_memory_bytes as f64 / GB
        )
    };
    let gpu = hardware.main_gpu().map_or_else(
        || "no GPU found, so models run on the CPU".to_owned(),
        |gpu| {
            format!(
                "{} with {:.1} GB of GPU memory",
                gpu.name,
                gpu.dedicated_memory_bytes as f64 / GB
            )
        },
    );
    format!(
        "This computer: {ram}, {gpu}. Read locally and never sent. Fit marks use the launcher's own memory rules at a {}-token context, with an estimated KV cache; llama.cpp measures again at launch.",
        launcher_context_tokens()
    )
}

/// The fit of each variant, `None` for projectors.
pub(crate) fn variant_fits(repo: &Repo, memory: &MemoryFacts) -> Vec<Option<LocalModelFit>> {
    repo.variants
        .iter()
        .map(|variant| {
            (variant.kind == VariantKind::Model).then(|| {
                local_model_fit(
                    variant.size_bytes(),
                    false,
                    launcher_context_tokens(),
                    memory,
                )
                .fit
            })
        })
        .collect()
}

/// The largest model that fits fully on the GPU, or else the largest that
/// runs with offload or on the CPU.
pub(crate) fn recommended_variant(repo: &Repo, fits: &[Option<LocalModelFit>]) -> Option<usize> {
    let best = |wanted: &[LocalModelFit]| {
        repo.variants
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                fits.get(*index)
                    .copied()
                    .flatten()
                    .is_some_and(|fit| wanted.contains(&fit))
            })
            .max_by_key(|(_, variant)| variant.size_bytes())
            .map(|(index, _)| index)
    };
    best(&[LocalModelFit::FullGpu])
        .or_else(|| best(&[LocalModelFit::Offload, LocalModelFit::CpuOnly]))
}

fn fit_tone(fit: LocalModelFit) -> &'static str {
    match fit {
        LocalModelFit::FullGpu => "ok",
        LocalModelFit::Offload | LocalModelFit::CpuOnly => "secondary",
        LocalModelFit::TooLarge => "danger",
    }
}

const IDLE_TEXT: &str = "Search Hugging Face for GGUF models to run with the local model add-on. Searches go to huggingface.co only when you press Search, and nothing about this computer is sent.";

fn token(secrets: &dyn SecretStore) -> Option<HubToken> {
    secrets
        .get(HUB_TOKEN_KEY)
        .ok()
        .flatten()
        .and_then(|value: SecretValue| HubToken::new(value.expose()))
}

fn take_busy(shared: &Mutex<Shared>) -> bool {
    let Ok(mut state) = shared.lock() else {
        return false;
    };
    if state.busy.is_some() {
        return false;
    }
    state.busy = Some(CancellationToken::new());
    true
}

fn release_busy(shared: &Mutex<Shared>) {
    if let Ok(mut state) = shared.lock() {
        state.busy = None;
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new().name(name.into()).spawn(work);
}

fn block_on<T>(
    future: impl std::future::Future<Output = Result<T, UpdateError>>,
) -> Result<T, UpdateError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| UpdateError("The request could not start.".into()))?
        .block_on(future)
}

fn run_download(
    token: Option<HubToken>,
    repo: &Repo,
    variant: &Variant,
    models_dir: &Path,
    cancel: &CancellationToken,
    weak: &Weak<MainWindow>,
) -> Result<PathBuf, UpdateError> {
    block_on(async {
        let hub = Hub::new(token)?;
        let mut last = (usize::MAX, u64::MAX, false);
        let download = hub.download(repo, variant, models_dir, |progress| {
            let percent = progress
                .received
                .saturating_mul(100)
                .checked_div(progress.total)
                .unwrap_or(100);
            if last == (progress.file, percent, progress.checking) {
                return;
            }
            last = (progress.file, percent, progress.checking);
            let part = if progress.files > 1 {
                format!(" part {} of {}", progress.file + 1, progress.files)
            } else {
                String::new()
            };
            let text = if progress.checking {
                format!(
                    "Checking the existing{part} file of {}, {percent}%.",
                    variant.label()
                )
            } else {
                format!(
                    "Downloading {}{part}, {percent}% of {}.",
                    variant.label(),
                    hub::size_text(progress.total)
                )
            };
            let _ = weak.upgrade_in_event_loop(move |window| {
                window.global::<App>().set_market_detail(text.into());
            });
        });
        tokio::select! {
            result = download => result,
            () = cancel.cancelled() => Err(UpdateError(
                "The download was cancelled. The part already downloaded is kept, so downloading again resumes. Use Discard to delete it.".into(),
            )),
        }
    })
}

fn show_error(window: &MainWindow, error_value: &UpdateError) {
    let app = window.global::<App>();
    app.set_market_error(false);
    let state = app.get_market_state();
    // Stay on the repository or results that were showing, with the reason.
    let back = match state.as_str() {
        "confirm" | "downloading" | "repo" => "repo",
        "loading" | "results" => "results",
        _ if app.get_market_results().row_count() > 0 => "results",
        _ => "idle",
    };
    app.set_market_state(back.into());
    app.set_market_error(true);
    app.set_market_detail(error_value.to_string().into());
}

fn count_text(value: u64) -> String {
    match value {
        0..=999 => value.to_string(),
        1_000..=999_999 => format!("{:.1}k", value as f64 / 1_000.0),
        _ => format!("{:.1}M", value as f64 / 1_000_000.0),
    }
}

pub(crate) fn result_detail(summary: &RepoSummary) -> String {
    let mut parts = vec![
        format!("{} downloads", count_text(summary.downloads)),
        format!("{} likes", count_text(summary.likes)),
        summary.license.clone().map_or_else(
            || "license not stated".into(),
            |license| format!("{license} license"),
        ),
    ];
    if summary.gated {
        parts.push("gated: needs a token and accepted terms".into());
    }
    parts.join(", ")
}

fn show_results(window: &MainWindow, query: &str, results: &[RepoSummary]) {
    let app = window.global::<App>();
    app.set_market_error(false);
    app.set_market_results(ModelRc::new(VecModel::from(
        results
            .iter()
            .map(|summary| MarketRepoView {
                id: summary.id.as_str().into(),
                detail: result_detail(summary).into(),
            })
            .collect::<Vec<_>>(),
    )));
    app.set_market_state("results".into());
    app.set_market_detail(
        if results.is_empty() {
            format!("No GGUF models on Hugging Face match \"{query}\".")
        } else {
            format!(
                "{} GGUF repositories, most downloaded first. Open one to see its files.",
                results.len()
            )
        }
        .into(),
    );
}

pub(crate) fn repo_detail(repo: &Repo) -> String {
    let mut parts = vec![repo.license.clone().map_or_else(
        || "License not stated".into(),
        |license| format!("{license} license"),
    )];
    if let Some(context) = repo.context_length {
        parts.push(format!("context length {context} tokens"));
    }
    if let Some(parameters) = repo.parameters {
        parts.push(format!("{:.1}B parameters", parameters as f64 / 1e9));
    }
    if let Some(architecture) = &repo.architecture {
        parts.push(format!("{architecture} architecture"));
    }
    parts.push(format!("commit {}", &repo.commit[..12]));
    if repo.gated {
        parts.push("gated: accept its terms on huggingface.co and set a token".into());
    }
    let mut text = parts.join(", ");
    if !repo.skipped_files.is_empty() {
        text.push_str(&format!(
            ". {} GGUF file(s) are not offered because Hugging Face publishes no checksum for them or their name or split set is incomplete.",
            repo.skipped_files.len()
        ));
    }
    text
}

pub(crate) fn variant_detail(variant: &Variant, models_dir: &Path) -> String {
    let files = variant
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let present = variant
        .files
        .iter()
        .all(|file| models_dir.join(&file.local_name).is_file());
    let mut text = format!("{}, {}", hub::size_text(variant.size_bytes()), files);
    if variant.kind == VariantKind::Projector {
        text.push_str(". For image input, listed with `mmproj` in the model's .adapters file");
    }
    if present {
        text.push_str(". In the model folder");
    }
    text
}

fn show_repo(window: &MainWindow, repo: &Repo, models_dir: &Path, has_token: bool) {
    let app = window.global::<App>();
    app.set_market_error(false);
    app.set_market_repo_title(repo.id.as_str().into());
    app.set_market_repo_detail(repo_detail(repo).into());
    let memory = MEMORY.get().cloned().unwrap_or_default();
    let fits = variant_fits(repo, &memory);
    let recommended = recommended_variant(repo, &fits);
    app.set_market_files(ModelRc::new(VecModel::from(
        repo.variants
            .iter()
            .enumerate()
            .map(|(index, variant)| {
                let fit = fits.get(index).copied().flatten();
                let recommended = recommended == Some(index);
                MarketFileView {
                    index: i32::try_from(index).unwrap_or(i32::MAX),
                    label: if recommended {
                        format!("{}, recommended for this computer", variant.label())
                    } else {
                        variant.label()
                    }
                    .into(),
                    detail: variant_detail(variant, models_dir).into(),
                    fit: fit.map_or("", LocalModelFit::label).into(),
                    fit_tone: fit.map_or("secondary", fit_tone).into(),
                }
            })
            .collect::<Vec<_>>(),
    )));
    app.set_market_state("repo".into());
    app.set_market_detail(
        if repo.variants.is_empty() {
            "This repository has no GGUF file Argentum can download and check.".into()
        } else if repo.gated && !has_token {
            "This model is gated. Accept its terms on huggingface.co and set a Hugging Face token before downloading.".into()
        } else {
            SharedString::default()
        },
    );
}

/// What the owner approves: every file, its size, where it comes from, how it
/// is checked, where it goes, and the disk space it needs.
pub(crate) fn confirm_text(
    repo: &Repo,
    variant: &Variant,
    plan: &hub::DownloadPlan,
    models_dir: &Path,
    has_token: bool,
) -> String {
    let mut lines = vec![format!(
        "{} from huggingface.co/{} at commit {}.",
        variant.label(),
        repo.id,
        &repo.commit[..12]
    )];
    for file in &variant.files {
        let state = if plan.present.contains(&file.local_name) {
            " Already in the model folder, checked again before it is kept."
        } else {
            ""
        };
        lines.push(format!(
            "{}: {}, checked against the SHA-256 Hugging Face publishes ({}...).{state}",
            file.path,
            hub::size_text(file.size_bytes),
            &file.sha256[..16]
        ));
    }
    lines.push(format!(
        "License: {}. Read it on the model page before you use the model.",
        repo.license.as_deref().unwrap_or("not stated")
    ));
    let space = match (plan.space_problem(), plan.free_bytes) {
        (Some(problem), _) => problem,
        (None, Some(free)) => format!(
            "Needs {} on disk, {} is free.",
            hub::size_text(plan.remaining_bytes),
            hub::size_text(free)
        ),
        (None, None) => format!(
            "Needs {} on disk. Free space could not be read.",
            hub::size_text(plan.remaining_bytes)
        ),
    };
    lines.push(space);
    if plan.resumed_bytes > 0 {
        lines.push(format!(
            "{} were downloaded earlier and will be resumed.",
            hub::size_text(plan.resumed_bytes)
        ));
    }
    if repo.gated && !has_token {
        lines.push("This model is gated, so the download needs a Hugging Face token and its accepted terms.".into());
    }
    lines.push(format!(
        "The files go to {}. A file that fails its check is deleted. Nothing runs until you start the local model, which asks for approval.",
        models_dir.display()
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> Repo {
        let payload = serde_json::json!({
            "id": "tester/Tiny-GGUF",
            "sha": "0123456789abcdef0123456789abcdef01234567",
            "gated": "manual",
            "cardData": { "license": "apache-2.0" },
            "gguf": { "total": 4_000_000_000_u64, "context_length": 32768, "architecture": "qwen3" },
            "siblings": [
                { "rfilename": "Tiny-Q4_K_M.gguf", "size": 2_500_000_000_u64, "lfs": { "sha256": "a".repeat(64), "size": 2_500_000_000_u64 } },
                { "rfilename": "mmproj-F16.gguf", "size": 600_000_000_u64, "lfs": { "sha256": "b".repeat(64), "size": 600_000_000_u64 } }
            ]
        });
        hub::parse_repo(&serde_json::to_vec(&payload).expect("json")).expect("repo")
    }

    #[test]
    fn the_repository_shows_license_context_and_gating() {
        let text = repo_detail(&repo());
        assert!(text.contains("apache-2.0 license"), "{text}");
        assert!(text.contains("context length 32768"), "{text}");
        assert!(text.contains("4.0B parameters"), "{text}");
        assert!(text.contains("gated"), "{text}");
        let models = std::env::temp_dir().join("argentum-market-none");
        let detail = variant_detail(&repo().variants[1], &models);
        assert!(
            detail.contains("600.0 MB") && detail.contains("mmproj"),
            "{detail}"
        );
    }

    #[test]
    fn the_approval_names_files_checks_space_and_folder() {
        let repo = repo();
        let models = std::env::temp_dir().join(format!("argentum-market-{}", std::process::id()));
        let plan = hub::DownloadPlan {
            present: Vec::new(),
            remaining_bytes: 2_500_000_000,
            resumed_bytes: 0,
            free_bytes: Some(100_000_000_000),
        };
        let text = confirm_text(&repo, &repo.variants[0], &plan, &models, false);
        assert!(text.contains("Tiny-Q4_K_M.gguf: 2.50 GB"), "{text}");
        assert!(text.contains("SHA-256 Hugging Face publishes"), "{text}");
        assert!(text.contains("commit 0123456789ab"), "{text}");
        assert!(
            text.contains("Needs 2.50 GB on disk, 100.00 GB is free"),
            "{text}"
        );
        assert!(text.contains("needs a Hugging Face token"), "{text}");
        assert!(text.contains(&models.display().to_string()), "{text}");
        let tight = hub::DownloadPlan {
            free_bytes: Some(1_000_000_000),
            ..plan
        };
        let text = confirm_text(&repo, &repo.variants[0], &tight, &models, true);
        assert!(text.contains("Not enough disk space"), "{text}");
        assert!(!text.contains("needs a Hugging Face token"), "{text}");
        assert!(
            !models.exists(),
            "describing a download must not create files"
        );
    }

    #[test]
    fn the_recommendation_is_the_largest_quant_that_fits() {
        let payload = serde_json::json!({
            "id": "tester/Fit-GGUF",
            "sha": "0123456789abcdef0123456789abcdef01234567",
            "siblings": [
                { "rfilename": "Fit-Q4_K_M.gguf", "size": 4_000_000_000_u64, "lfs": { "sha256": "a".repeat(64), "size": 4_000_000_000_u64 } },
                { "rfilename": "Fit-Q8_0.gguf", "size": 8_000_000_000_u64, "lfs": { "sha256": "b".repeat(64), "size": 8_000_000_000_u64 } },
                { "rfilename": "Fit-F16.gguf", "size": 16_000_000_000_u64, "lfs": { "sha256": "c".repeat(64), "size": 16_000_000_000_u64 } },
                { "rfilename": "mmproj-F16.gguf", "size": 600_000_000_u64, "lfs": { "sha256": "d".repeat(64), "size": 600_000_000_u64 } }
            ]
        });
        let repo = hub::parse_repo(&serde_json::to_vec(&payload).expect("json")).expect("repo");
        let gib = 1024 * 1024 * 1024;
        let pc = MemoryFacts {
            system_memory_bytes: 32 * gib,
            gpu_memory_bytes: Some(8 * gib),
        };
        let fits = variant_fits(&repo, &pc);
        assert_eq!(
            fits,
            [
                Some(LocalModelFit::FullGpu),
                Some(LocalModelFit::Offload),
                Some(LocalModelFit::Offload),
                None
            ]
        );
        assert_eq!(recommended_variant(&repo, &fits), Some(0));
        let small = MemoryFacts {
            system_memory_bytes: 8 * gib,
            gpu_memory_bytes: None,
        };
        let fits = variant_fits(&repo, &small);
        assert_eq!(fits[2], Some(LocalModelFit::TooLarge));
        assert_eq!(recommended_variant(&repo, &fits), None, "{fits:?}");
    }

    #[test]
    fn the_hardware_line_names_memory_and_gpu() {
        let text = hardware_text(&HardwareInfo {
            system_memory_bytes: 34_359_738_368,
            gpus: vec![argentum_platform::GpuInfo {
                name: "Test GPU".into(),
                dedicated_memory_bytes: 8_589_934_592,
            }],
        });
        assert!(text.contains("34 GB of system memory"), "{text}");
        assert!(text.contains("Test GPU with 8.6 GB"), "{text}");
        assert!(text.contains("never sent"), "{text}");
        assert!(hardware_text(&HardwareInfo::default()).contains("no GPU found"));
    }

    #[test]
    fn search_results_read_plainly() {
        let summary = RepoSummary {
            id: "a/b".into(),
            downloads: 1_234_567,
            likes: 999,
            license: None,
            gated: true,
        };
        let text = result_detail(&summary);
        assert_eq!(
            text,
            "1.2M downloads, 999 likes, license not stated, gated: needs a token and accepted terms"
        );
    }
}
