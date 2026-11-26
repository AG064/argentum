// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The marketplace's opt-in speed report.
//!
//! "Measure speed" times one fixed prompt on the local model that is already
//! running, on this computer. Argentum then shows the exact report it would
//! share: repository, quantization, GPU model, RAM size and tokens per second,
//! nothing else. "Open report on GitHub" opens a prefilled issue in the
//! owner's browser, where they submit it or close it. Argentum never sends the
//! report itself, and nothing is measured or shown without a button press.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use argentum_cli::LocalCommandHandle;
use argentum_platform::HardwareInfo;
use argentum_providers::LocalSpeedMeasurement;
use argentum_ui::{App, UiHandle};
use argentum_update::hub::{quant_of, valid_repo_id};
use argentum_update::speeds::{SpeedReport, REPORT_ISSUE_URL};
use serde::{Deserialize, Serialize};
use slint::ComponentHandle;
use tokio_util::sync::CancellationToken;

/// Where marketplace downloads record which repository a file came from.
const SOURCES_FILE: &str = "marketplace-sources.json";

pub(crate) struct Context {
    pub(crate) local: LocalCommandHandle,
    pub(crate) runtime: tokio::runtime::Handle,
    pub(crate) models_dir: Arc<PathBuf>,
    pub(crate) hardware: HardwareInfo,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Sources {
    /// Local file name to Hugging Face repository.
    files: std::collections::BTreeMap<String, String>,
}

/// Remember that `local_name` was downloaded from `repo`, so a later speed
/// report can name the repository. A failure only loses that convenience.
pub(crate) fn record_source(models_dir: &Path, local_name: &str, repo: &str) {
    let path = models_dir.join(SOURCES_FILE);
    let mut sources: Sources = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    sources.files.insert(local_name.to_owned(), repo.to_owned());
    if let Ok(bytes) = serde_json::to_vec_pretty(&sources) {
        let _ = std::fs::write(path, bytes);
    }
}

/// The repository a local model file came from: a marketplace download, or
/// one of the add-on's pinned Hugging Face models.
pub(crate) fn source_of(models_dir: &Path, local_name: &str) -> Option<String> {
    let recorded = std::fs::read(models_dir.join(SOURCES_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Sources>(&bytes).ok())
        .and_then(|sources| sources.files.get(local_name).cloned());
    recorded
        .or_else(|| {
            argentum_update::addon::model(local_name).and_then(|model| {
                let path = model.artifact.url.strip_prefix("https://huggingface.co/")?;
                let mut parts = path.split('/');
                Some(format!("{}/{}", parts.next()?, parts.next()?))
            })
        })
        .filter(|repo| valid_repo_id(repo))
}

/// System memory in whole GB as Windows shows it (binary gigabytes, rounded).
pub(crate) fn ram_gb(bytes: u64) -> u32 {
    let gib = (bytes as f64 / (1024.0 * 1024.0 * 1024.0)).round();
    gib.clamp(0.0, f64::from(u32::MAX)) as u32
}

/// The report for a measurement, or why there cannot be one.
pub(crate) fn report_for(
    measurement: &LocalSpeedMeasurement,
    models_dir: &Path,
    hardware: &HardwareInfo,
) -> Result<SpeedReport, String> {
    let file = Path::new(&measurement.model).file_name().map_or_else(
        || measurement.model.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    let repo = source_of(models_dir, &file).ok_or_else(|| {
        format!("{file} did not come from the marketplace or the add-on list, so Argentum cannot say which Hugging Face repository it is. A report needs that.")
    })?;
    let quant = quant_of(&file)
        .ok_or_else(|| format!("The quantization of {file} cannot be read from its name."))?;
    let gpu = hardware
        .main_gpu()
        .map_or_else(|| "CPU only".to_owned(), |gpu| gpu.name.clone());
    Ok(SpeedReport {
        repo,
        quant,
        gpu,
        ram_gb: ram_gb(hardware.system_memory_bytes),
        tokens_per_second: measurement.tokens_per_second,
    })
}

/// The text shown before anything can be shared: the exact data, and that
/// nothing has been sent.
pub(crate) fn report_text(
    measurement: &LocalSpeedMeasurement,
    report: &Result<SpeedReport, String>,
) -> String {
    let measured = format!(
        "Measured on this computer: {:.1} tokens per second ({} tokens in {:.1} s) for {} at a {}-token context. Nothing has been sent.",
        measurement.tokens_per_second,
        measurement.output_tokens,
        measurement.generation_seconds,
        measurement.model,
        measurement.context_tokens
    );
    match report {
        Ok(report) => format!(
            "{measured}\n\nIf you choose to share it, this is the whole report:\n\n{}\n\n\"Open report on GitHub\" opens a prefilled issue in your browser. You submit it there, or close it. Argentum does not send it.",
            report.entry_json()
        ),
        Err(reason) => format!("{measured}\n\n{reason}"),
    }
}

#[derive(Default)]
struct Shared {
    report: Option<SpeedReport>,
    busy: Option<CancellationToken>,
}

pub(crate) fn connect(ui: &UiHandle, context: Context) {
    let app = ui.window().global::<App>();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let context = Arc::new(context);
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        let context = context.clone();
        app.on_market_measure(move || {
            let Some(window) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            if let Some(cancel) = state.busy.take() {
                cancel.cancel();
                window.global::<App>().set_market_report_state("".into());
                return;
            }
            let cancel = CancellationToken::new();
            state.busy = Some(cancel.clone());
            state.report = None;
            drop(state);
            let app = window.global::<App>();
            app.set_market_report_state("measuring".into());
            app.set_market_report_text(
                "Measuring the running local model with one fixed prompt. This runs only on this computer.".into(),
            );
            let weak = weak.clone();
            let shared = shared.clone();
            let context = context.clone();
            context.runtime.clone().spawn(async move {
                let result = context.local.measure_local_speed(cancel.clone()).await;
                let cancelled = cancel.is_cancelled();
                let (text, report) = match result {
                    Ok(measurement) => {
                        let report = report_for(&measurement, &context.models_dir, &context.hardware);
                        (report_text(&measurement, &report), report.ok())
                    }
                    Err(_) => (
                        "No local model is running. Choose llama.cpp in Providers, start a model and approve it, then measure again.".to_owned(),
                        None,
                    ),
                };
                let ready = report.is_some();
                if let Ok(mut state) = shared.lock() {
                    state.busy = None;
                    state.report = report;
                }
                if cancelled {
                    return;
                }
                let _ = weak.upgrade_in_event_loop(move |window| {
                    let app = window.global::<App>();
                    app.set_market_report_state(if ready { "ready" } else { "shown" }.into());
                    app.set_market_report_text(text.into());
                });
            });
        });
    }
    {
        let weak = ui.weak_window();
        let shared = shared.clone();
        app.on_market_open_report(move || {
            let Some(window) = weak.upgrade() else { return };
            let Some(report) = shared.lock().ok().and_then(|state| state.report.clone()) else {
                return;
            };
            let app = window.global::<App>();
            if open_in_browser(&report.issue_url()) {
                app.set_market_report_text(
                    "The prefilled issue is open in your browser. Submit it there if you want to share it. Argentum sent nothing."
                        .into(),
                );
                app.set_market_report_state("shown".into());
            } else {
                app.set_market_report_text(
                    format!(
                        "The browser could not be opened. To share the report, open {REPORT_ISSUE_URL} and paste:\n\n{}",
                        report.entry_json()
                    )
                    .into(),
                );
            }
        });
    }
    {
        let weak = ui.weak_window();
        app.on_market_close_report(move || {
            let Some(window) = weak.upgrade() else { return };
            if let Ok(mut state) = shared.lock() {
                state.report = None;
            }
            let app = window.global::<App>();
            app.set_market_report_state("".into());
            app.set_market_report_text("".into());
        });
    }
}

/// Open the project's own new-issue page in the default browser. Any other
/// address is refused.
fn open_in_browser(url: &str) -> bool {
    if !url.starts_with(&format!("{REPORT_ISSUE_URL}?")) {
        return false;
    }
    #[cfg(windows)]
    {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let operation = HSTRING::from("open");
        let target = HSTRING::from(url);
        // SAFETY: both strings are NUL-terminated and live across the call.
        // ShellExecuteW opens the https address with the default browser.
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        result.0 as isize > 32
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(model: &str) -> LocalSpeedMeasurement {
        LocalSpeedMeasurement {
            model: model.into(),
            context_tokens: 16384,
            prompt_tokens: 20,
            output_tokens: 128,
            generation_seconds: 3.2,
            tokens_per_second: 40.0,
        }
    }

    fn hardware() -> HardwareInfo {
        HardwareInfo {
            system_memory_bytes: 34_276_524_032,
            gpus: vec![argentum_platform::GpuInfo {
                name: "Test GPU 8GB".into(),
                dedicated_memory_bytes: 8_589_934_592,
            }],
        }
    }

    #[test]
    fn a_report_names_the_repo_from_the_marketplace_record_or_the_add_on() {
        let models = std::env::temp_dir().join(format!("argentum-speed-{}", std::process::id()));
        std::fs::create_dir_all(&models).expect("models");
        record_source(&models, "Tiny-Q5_K_M.gguf", "tester/Tiny-GGUF");
        let report =
            report_for(&measurement("Tiny-Q5_K_M.gguf"), &models, &hardware()).expect("report");
        assert_eq!(report.repo, "tester/Tiny-GGUF");
        assert_eq!(report.quant, "Q5_K_M");
        assert_eq!(report.gpu, "Test GPU 8GB");
        assert_eq!(report.ram_gb, 32);
        let pinned = report_for(&measurement("Qwen3.5-4B-Q4_K_M.gguf"), &models, &hardware())
            .expect("add-on");
        assert_eq!(pinned.repo, "unsloth/Qwen3.5-4B-GGUF");
        let unknown = report_for(&measurement("Mine-Q4_0.gguf"), &models, &hardware());
        assert!(unknown.is_err());
        let _ = std::fs::remove_dir_all(&models);
    }

    #[test]
    fn the_shown_text_holds_the_exact_report_and_says_nothing_was_sent() {
        let measured = measurement("Qwen3.5-4B-Q4_K_M.gguf");
        let models = std::env::temp_dir().join("argentum-speed-none");
        let report = report_for(&measured, &models, &hardware());
        let text = report_text(&measured, &report);
        let entry = report.expect("report").entry_json();
        assert!(text.contains(&entry), "{text}");
        assert!(text.contains("Nothing has been sent"), "{text}");
        assert!(text.contains("Argentum does not send it"), "{text}");
        assert!(!models.exists());
    }

    #[test]
    fn only_the_project_issue_page_can_be_opened() {
        assert!(!open_in_browser("https://example.com/?x"));
        assert!(!open_in_browser(
            "https://github.com/AG064/argentum/issues/newx?a"
        ));
    }
}
