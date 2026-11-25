// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Settings > Providers > Local model add-on. The Windows release does not
//! include llama.cpp or any model. The owner reviews exactly what would be
//! downloaded and confirms it here, and can remove it again.
//!
//! Like updates, this belongs to the desktop program rather than the command
//! host: no tool, model, or protocol client can reach it. Each download starts
//! only from the "Download and install" button, after the owner has seen the
//! files, sizes, sources, and checks. Nothing here starts a process. The local
//! runtime still asks for its own launch approval.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use argentum_cli::LocalCommandHandle;
use argentum_platform::AppPaths;
use argentum_providers::{
    local_runtime_bundle_path, local_runtime_offers, runtime_bundle_is_valid,
};
use argentum_ui::{AddonModelView, App, MainWindow, UiHandle};
use argentum_update::addon::{self, Layout, ModelPin, Step};
use argentum_update::UpdateError;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use tokio_util::sync::CancellationToken;

pub(crate) struct Context {
    pub(crate) local: LocalCommandHandle,
    pub(crate) runtime: tokio::runtime::Handle,
}

/// The add-on's paths, which are the ones the local runtime already reads.
pub(crate) fn layout(data_dir: &Path) -> Option<Layout> {
    let offer = local_runtime_offers().first()?;
    let runtimes = data_dir.join("runtimes");
    Some(Layout {
        runtime_bundle: local_runtime_bundle_path(&runtimes, offer).ok()?,
        runtime_dir: runtimes.join("llama-cpp"),
        models_dir: data_dir.join("models"),
        work_dir: data_dir.join("addons"),
    })
}

#[derive(Default)]
struct Shared {
    chosen: Option<&'static ModelPin>,
    /// Set while an install or removal runs. Cancelling an install drops it,
    /// which rolls back whatever it had added.
    busy: Option<CancellationToken>,
}

pub(crate) fn connect(ui: &UiHandle, context: Context) {
    let window = ui.window();
    let Some(layout) = cfg!(windows)
        .then(AppPaths::discover)
        .and_then(Result::ok)
        .and_then(|paths| layout(&paths.data_dir))
    else {
        window.global::<App>().set_addon_supported(false);
        return;
    };
    let app = window.global::<App>();
    app.set_addon_supported(true);
    app.set_addon_models(ModelRc::new(VecModel::from(
        addon::MODELS
            .iter()
            .map(|model| AddonModelView {
                id: model.id.into(),
                label: model.label.into(),
                detail: model_detail_text(model).into(),
            })
            .collect::<Vec<_>>(),
    )));
    // An install the app was closed or killed during is rolled back here.
    let recovered = addon::recover(&layout);
    show_status(window, &layout);
    if let Err(error_value) = recovered {
        show_error(window, &error_value);
    }

    let layout = Arc::new(layout);
    let shared = Arc::new(Mutex::new(Shared::default()));

    {
        let weak = ui.weak_window();
        let layout = layout.clone();
        let shared = shared.clone();
        app.on_addon_choose(move |id| {
            let (Some(window), Some(model)) = (weak.upgrade(), addon::model(&id)) else {
                return;
            };
            let Ok(mut state) = shared.lock() else { return };
            if state.busy.is_some() {
                return;
            }
            state.chosen = Some(model);
            let app = window.global::<App>();
            app.set_addon_confirm_text(confirm_text(&layout, model).into());
            app.set_addon_state("confirm".into());
        });
    }
    {
        let weak = ui.weak_window();
        let layout = layout.clone();
        let shared = shared.clone();
        app.on_addon_cancel(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            match &state.busy {
                // The install thread reports the cancellation once it has
                // rolled back.
                Some(token) => token.cancel(),
                None => {
                    state.chosen = None;
                    show_status(&window, &layout);
                }
            }
        });
    }
    {
        let weak = ui.weak_window();
        let layout = layout.clone();
        let shared = shared.clone();
        app.on_addon_install(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            let Some(model) = state.chosen else { return };
            if state.busy.is_some() {
                return;
            }
            let token = CancellationToken::new();
            state.busy = Some(token.clone());
            drop(state);
            let app = window.global::<App>();
            app.set_addon_state("installing".into());
            app.set_addon_detail("Checking the publisher's signature.".into());
            let weak = weak.clone();
            let layout = layout.clone();
            let thread_shared = shared.clone();
            // Hashing a multi-gigabyte model blocks, so the install gets its own
            // thread rather than a worker of the shared host runtime.
            let spawned = std::thread::Builder::new()
                .name("local-addon-install".into())
                .spawn(move || {
                    let result = run_install(&layout, model, &token, &weak);
                    if let Ok(mut state) = thread_shared.lock() {
                        state.busy = None;
                        state.chosen = None;
                    }
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        show_status(&window, &layout);
                        match result {
                            Ok(()) => window.global::<App>().set_addon_detail(
                                format!(
                                    "Installed {}. Choose llama.cpp in Providers and pick this model. Starting it asks for your approval.",
                                    model.label
                                )
                                .into(),
                            ),
                            Err(error_value) => show_error(&window, &error_value),
                        }
                    });
                });
            if spawned.is_err() {
                if let Ok(mut state) = shared.lock() {
                    state.busy = None;
                }
                show_error(
                    &window,
                    &UpdateError("The download could not start.".into()),
                );
            }
        });
    }
    {
        let weak = ui.weak_window();
        let layout = layout.clone();
        let shared = shared.clone();
        let local = context.local.clone();
        let runtime = context.runtime.clone();
        app.on_addon_remove(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            if state.busy.is_some() {
                return;
            }
            state.busy = Some(CancellationToken::new());
            drop(state);
            window.global::<App>().set_addon_state("removing".into());
            window
                .global::<App>()
                .set_addon_detail("Stopping the local model and removing the add-on.".into());
            let weak = weak.clone();
            let layout = layout.clone();
            let shared = shared.clone();
            let local = local.clone();
            runtime.spawn(async move {
                // A running llama-server keeps its files open, so stop it first.
                let stopped = local.unload_local_model().await;
                let layout_for_removal = layout.clone();
                let removed = tokio::task::spawn_blocking(move || addon::remove(&layout_for_removal))
                    .await
                    .unwrap_or_else(|_| Err(UpdateError("The removal stopped early.".into())));
                if let Ok(mut state) = shared.lock() {
                    state.busy = None;
                }
                let _ = weak.upgrade_in_event_loop(move |window| {
                    show_status(&window, &layout);
                    match (stopped, removed) {
                        (_, Ok(())) => window
                            .global::<App>()
                            .set_addon_detail(
                                "The add-on is removed. Your other model files were kept.".into(),
                            ),
                        (Err(_), Err(_)) => show_error(
                            &window,
                            &UpdateError(
                                "The local model could not be stopped, so its files are still in use. Restart Argentum and remove the add-on again."
                                    .into(),
                            ),
                        ),
                        (Ok(()), Err(error_value)) => show_error(&window, &error_value),
                    }
                });
            });
        });
    }
}

fn run_install(
    layout: &Layout,
    model: &'static ModelPin,
    token: &CancellationToken,
    weak: &Weak<MainWindow>,
) -> Result<(), UpdateError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| UpdateError("The download could not start.".into()))?;
    runtime.block_on(async {
        let source = addon::Network::new()?;
        let offer = local_runtime_offers()
            .first()
            .copied()
            .ok_or_else(|| UpdateError("This build has no local runtime pin.".into()))?;
        let mut last = (None::<&'static str>, u64::MAX);
        let install = addon::install(
            layout,
            model,
            &source,
            |staged| {
                // The local runtime's own archive checks, which it repeats before
                // every launch: size, SHA-256, layout, and the launcher's SHA-256.
                if runtime_bundle_is_valid(staged, &offer) {
                    Ok(())
                } else {
                    Err(UpdateError(
                        "The runtime archive is not the expected llama.cpp build, so it was discarded."
                            .into(),
                    ))
                }
            },
            |step, received, total| {
                let text = match step {
                    Step::CheckingSignature => {
                        "Checking the publisher's signature.".to_owned()
                    }
                    Step::Downloading(label) => {
                        let percent = total
                            .filter(|total| *total > 0)
                            .map_or(0, |total| received.saturating_mul(100) / total);
                        if last == (Some(label), percent) {
                            return;
                        }
                        last = (Some(label), percent);
                        format!("Downloading {label}, {percent}%.")
                    }
                    Step::Placing => "Checks passed. Putting the files in place.".to_owned(),
                };
                let _ = weak.upgrade_in_event_loop(move |window| {
                    window.global::<App>().set_addon_detail(text.into());
                });
            },
        );
        tokio::select! {
            result = install => result,
            () = token.cancelled() => Err(UpdateError(
                "The download was cancelled. Nothing was installed.".into(),
            )),
        }
    })
}

fn show_status(window: &MainWindow, layout: &Layout) {
    let status = addon::status(layout);
    let app = window.global::<App>();
    if status.installed() {
        let names = status
            .models
            .iter()
            .map(|model| model.label)
            .collect::<Vec<_>>()
            .join(", ");
        app.set_addon_state("installed".into());
        app.set_addon_detail(format!("Installed: llama.cpp runtime with {names}.").into());
    } else {
        app.set_addon_state("not-installed".into());
        app.set_addon_detail(SharedString::from(not_installed_text(&status)));
    }
}

fn not_installed_text(status: &addon::Status) -> &'static str {
    if status.runtime_installed {
        "The llama.cpp runtime is installed, but no model from the list is. Review a model to add it."
    } else {
        "Local llama.cpp inference is an optional add-on and is not installed. Argentum works without it. Review a model to see exactly what would be downloaded."
    }
}

fn show_error(window: &MainWindow, error_value: &UpdateError) {
    let app = window.global::<App>();
    app.set_addon_state("error".into());
    app.set_addon_detail(error_value.to_string().into());
}

fn size_text(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    let megabytes = bytes as f64 / MB;
    if megabytes >= 1000.0 {
        format!("{:.2} GB", megabytes / 1000.0)
    } else {
        format!("{megabytes:.1} MB")
    }
}

fn host_of(url: &str) -> &str {
    url.trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default()
}

fn model_detail_text(model: &ModelPin) -> String {
    format!(
        "{} model files, {}, {}, {}",
        model.artifacts().count(),
        size_text(model.total_size_bytes()),
        model.license,
        model.source
    )
}

/// What the owner approves: every file, its size, where it comes from, how it
/// is checked, and where it goes.
fn confirm_text(layout: &Layout, model: &ModelPin) -> String {
    let status = addon::status(layout);
    let runtime = addon::LLAMA_CPP_RUNTIME;
    let mut lines = Vec::new();
    if !status.runtime_installed {
        lines.push(format!(
            "{}: {}, {} license, from {}. Its SHA-256 is pinned in Argentum, and it must also be listed in llama.cpp's signed build attestation.",
            runtime.label,
            size_text(runtime.artifact.size_bytes),
            runtime.license,
            host_of(runtime.artifact.url)
        ));
    }
    let model_installed = status
        .models
        .iter()
        .any(|installed| installed.id == model.id);
    if !model_installed {
        for artifact in model.artifacts() {
            lines.push(format!(
                "{} ({}): {}, {} license, from {} ({}). Its SHA-256 is pinned in Argentum. Model files have no publisher signature to check.",
                model.label,
                artifact.file_name,
                size_text(artifact.size_bytes),
                model.license,
                host_of(artifact.url),
                model.source
            ));
        }
    }
    if lines.is_empty() {
        return format!(
            "{} is already installed. Nothing needs to be downloaded.",
            model.label
        );
    }
    let file_count = (if status.runtime_installed { 0 } else { 1 })
        + if model_installed {
            0
        } else {
            model.artifacts().count()
        };
    let total = (!status.runtime_installed)
        .then_some(runtime.artifact.size_bytes)
        .into_iter()
        .chain(
            (!model_installed)
                .then(|| model.artifacts().map(|artifact| artifact.size_bytes))
                .into_iter()
                .flatten(),
        )
        .fold(0_u64, u64::saturating_add);
    format!(
        "Argentum may download up to {} over HTTPS, up to {} total. Existing files are reused only when their pinned checks pass.\n\n{}\n\nThe files go to {}. If a check fails or the download stops, downloaded files are removed. Nothing runs until you start the local model, which asks for approval.",
        if file_count == 1 { "1 file".to_owned() } else { format!("{file_count} files") },
        size_text(total),
        lines.join("\n"),
        display_root(layout).display()
    )
}

fn display_root(layout: &Layout) -> PathBuf {
    layout
        .work_dir
        .parent()
        .map_or_else(|| layout.work_dir.clone(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_add_on_uses_the_paths_and_pins_the_local_runtime_reads() {
        let data = std::env::temp_dir().join("argentum-addon-layout");
        let layout = layout(&data).expect("layout");
        let offer = local_runtime_offers()[0];
        assert_eq!(
            layout.runtime_bundle,
            local_runtime_bundle_path(&data.join("runtimes"), &offer).expect("bundle path")
        );
        assert!(layout.runtime_bundle.starts_with(&layout.runtime_dir));
        assert_eq!(layout.models_dir, data.join("models"));

        let pin = addon::LLAMA_CPP_RUNTIME.artifact;
        assert_eq!(pin.url, offer.download_url);
        assert_eq!(pin.file_name, offer.filename);
        assert_eq!(pin.size_bytes, offer.size_bytes);
        assert_eq!(pin.sha256, offer.sha256);

        // The model the command line offers is on the add-on list with the same pin.
        for offer in argentum_providers::local_model_offers() {
            let model = addon::model(offer.id).expect("listed");
            assert_eq!(model.artifact.url, offer.download_url);
            assert_eq!(model.artifact.size_bytes, offer.size_bytes);
            assert_eq!(model.artifact.sha256, offer.sha256);
        }

        let inkling = addon::model("Inkling-2-Mini-AD-Q4_K.gguf").expect("Inkling pin");
        // The adapter list the add-on writes is one the local runtime accepts,
        // and names only files downloaded with the model.
        let models = std::env::temp_dir().join(format!(
            "argentum-addon-adapters-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&models).expect("models folder");
        std::fs::write(
            argentum_providers::local_model_adapters_path(&models, inkling.id),
            inkling.adapter_list.expect("Inkling adapter list"),
        )
        .expect("write list");
        let adapters =
            argentum_providers::local_model_adapters(&models, inkling.id).expect("valid list");
        let _ = std::fs::remove_dir_all(&models);
        assert_eq!(adapters.len(), inkling.additional_artifacts.len());
        for adapter in adapters {
            assert!(inkling
                .additional_artifacts
                .iter()
                .any(|artifact| artifact.file_name == adapter.filename));
        }
    }

    #[test]
    fn the_approval_text_names_every_file_and_check() {
        let data = std::env::temp_dir().join(format!("argentum-addon-text-{}", std::process::id()));
        let layout = layout(&data).expect("layout");
        let text = confirm_text(&layout, &addon::MODELS[1]);
        assert!(text.contains("2 files"), "{text}");
        assert!(text.contains("github.com"));
        assert!(text.contains("signed build attestation"));
        assert!(text.contains("no publisher signature"));
        assert!(text.contains(addon::MODELS[1].label));
        assert!(text.contains("downloaded files are removed"));
        assert!(
            !data.exists(),
            "describing a download must not create files"
        );
    }

    #[test]
    fn the_inkling_approval_lists_the_complete_model_set_and_gpu_mode() {
        let data =
            std::env::temp_dir().join(format!("argentum-inkling-confirm-{}", std::process::id()));
        let layout = layout(&data).expect("layout");
        let model = addon::model("Inkling-2-Mini-AD-Q4_K.gguf").expect("Inkling model");
        let text = confirm_text(&layout, model);
        assert!(model_detail_text(model).contains("3 model files, 17.21 GB"));
        assert!(text.contains("up to 4 files"), "{text}");
        assert!(text.contains("up to 17.25 GB"), "{text}");
        assert!(text.contains("Inkling-2-Mini-AD-Q4_K.gguf"), "{text}");
        assert!(
            text.contains("Inkling-2-Mini-Uncensored-LoRA.gguf"),
            "{text}"
        );
        assert!(text.contains("mmproj-Inkling-2-Mini-F16.gguf"), "{text}");
        assert!(text.contains("Hugging Face, AtomicChat/Inkling-2-Mini-GGUF"));
        assert!(text.contains("SHA-256 is pinned in Argentum"));
        assert_eq!(model.artifacts().count(), 3);
        assert_eq!(model.total_size_bytes(), 17_208_713_408);
        assert!(
            !data.exists(),
            "describing a download must not create files"
        );
    }

    #[test]
    fn the_not_installed_state_says_so() {
        let status = addon::Status {
            runtime_installed: false,
            models: Vec::new(),
        };
        assert!(not_installed_text(&status).contains("not installed"));
        assert!(not_installed_text(&status).contains("works without it"));
    }

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size_text(34_403_304), "34.4 MB");
        assert_eq!(size_text(2_740_937_888), "2.74 GB");
    }
}
