//! The workbench actions that belong to the desktop program rather than a command:
//! installing an update and reopening in another workspace. They act on the
//! installed program, so no tool, model, or protocol client can reach them.

use std::env;
use std::path::{Path, PathBuf};

use argentum_platform::AppPaths;
use argentum_ui::{App, UiHandle};
use slint::{ComponentHandle, SharedString};

pub(crate) fn connect(ui: &UiHandle, app_paths: &AppPaths, runtime: &tokio::runtime::Handle) {
    connect_updates(ui, app_paths, runtime);
    ui.window().global::<App>().on_report_problem(|| {
        // A fixed github.com address; explorer hands it to the default browser.
        let url = argentum_app::issue_report_url(std::env::consts::OS);
        if let Err(error) = std::process::Command::new("explorer.exe").arg(&url).spawn() {
            tracing::warn!(%error, "could not open the issue page");
        }
    });

    connect_folder_picker(ui);

    let paths = app_paths.clone();
    let reopen_ui = ui.weak_window();
    ui.window().global::<App>().on_open_workspace(move |path| {
        let Some(window) = reopen_ui.upgrade() else {
            return;
        };
        match paths.save_workspace(Path::new(path.trim())) {
            Ok(_) => match env::current_exe().and_then(|executable| {
                std::process::Command::new(executable)
                    .env_remove("ARGENTUM_WORKSPACE")
                    .spawn()
            }) {
                Ok(_) => {
                    let _ = slint::quit_event_loop();
                }
                Err(_) => window.set_workspace_notice(
                    "The folder is saved. Restart Argentum to open it.".into(),
                ),
            },
            Err(_) => {
                window.set_workspace_notice("That folder cannot be opened.".into());
            }
        }
    });
}

/// Settings > Workspace > Choose folder. The picker only fills in the path
/// after the same check a typed path gets; Open and restart still saves it.
#[cfg(windows)]
fn connect_folder_picker(ui: &UiHandle) {
    use argentum_app::windows_dialogs::{pick_folder, DialogError};
    use argentum_app::workspace_choice::{
        check_picked_folder, display_path, WorkspaceChoice, PICKER_FAILED,
    };

    let app = ui.window().global::<App>();
    app.set_workspace_picker_available(true);
    let weak = ui.weak_window();
    app.on_choose_workspace_folder(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let app = window.global::<App>();
        let picked = match pick_folder("Choose a workspace folder") {
            Ok(path) => Some(path),
            Err(DialogError::Cancelled) => None,
            Err(DialogError::Failed) => {
                app.set_workspace_notice(PICKER_FAILED.into());
                return;
            }
        };
        match check_picked_folder(picked.as_deref()) {
            WorkspaceChoice::Cancelled => {}
            WorkspaceChoice::Ready(path) => {
                app.set_workspace_draft(display_path(&path).into());
                app.set_workspace_notice(SharedString::default());
            }
            WorkspaceChoice::Refused(reason) => app.set_workspace_notice(reason.into()),
        }
    });
}

#[cfg(not(windows))]
fn connect_folder_picker(_ui: &UiHandle) {}

/// The Windows Save dialog for an in-app session export. The file is written
/// only after the owner chooses where.
#[cfg(windows)]
pub(crate) fn session_export_sink() -> Option<argentum_app::session_export::SessionExportSink> {
    use argentum_app::session_export::ExportDelivery;
    use argentum_app::windows_dialogs::{save_json_file, DialogError};

    Some(Box::new(|export| {
        match save_json_file("Export session", &export.file_name) {
            Ok(path) => std::fs::write(&path, &export.json)
                .map(|()| ExportDelivery::Saved(path.display().to_string()))
                .map_err(|_| {
                    "Argentum could not write that file. Choose another place.".to_owned()
                }),
            Err(DialogError::Cancelled) => Ok(ExportDelivery::Cancelled),
            Err(DialogError::Failed) => Err("The Save dialog could not open.".to_owned()),
        }
    }))
}

#[cfg(not(windows))]
pub(crate) fn session_export_sink() -> Option<argentum_app::session_export::SessionExportSink> {
    None
}

/// Updates belong to this connector rather than the command host: they act on the
/// installed program, not a workspace, so no tool, model, or protocol client can
/// reach them. Each step starts only from a click in Settings.
fn connect_updates(ui: &UiHandle, app_paths: &AppPaths, runtime: &tokio::runtime::Handle) {
    let window = ui.window();
    let app = window.global::<App>();
    let executable = env::current_exe().ok();
    if let Some(executable) = &executable {
        argentum_update::clean_after_update(executable);
    }
    window.set_update_current(argentum_update::CURRENT_VERSION.into());
    window.set_update_install_blocker(
        executable
            .as_deref()
            .and_then(argentum_update::install_blocker)
            .unwrap_or_default()
            .into(),
    );
    let release = std::sync::Arc::new(std::sync::Mutex::new(None::<argentum_update::Release>));
    let verified = std::sync::Arc::new(std::sync::Mutex::new(None::<PathBuf>));

    {
        let weak = ui.weak_window();
        let handle = runtime.clone();
        let release = release.clone();
        app.on_update_check(move || {
            let weak = weak.clone();
            let release = release.clone();
            if let Some(window) = weak.upgrade() {
                window.set_update_state("checking".into());
                window.set_update_message(SharedString::default());
            }
            handle.spawn(async move {
                let result = argentum_update::check_latest().await;
                let _ = weak.upgrade_in_event_loop(move |window| match result {
                    Ok(found) => {
                        if argentum_update::is_newer(&found.version, argentum_update::CURRENT_VERSION) {
                            window.set_update_state(
                                if found.executable_url.is_some() && found.checksum_url.is_some() {
                                    "available"
                                } else {
                                    "error"
                                }
                                .into(),
                            );
                            window.set_update_version(found.version.clone().into());
                            window.set_update_page(found.page.clone().into());
                            window.set_update_message(
                                if found.executable_url.is_some() && found.checksum_url.is_some() {
                                    SharedString::default()
                                } else {
                                    format!(
                                        "Version {} has no Windows update package. You can get it from {}.",
                                        found.version, found.page
                                    )
                                    .into()
                                },
                            );
                            argentum_ui::workbench::set_update_notes(&window, &found.notes);
                            if let Ok(mut slot) = release.lock() {
                                *slot = Some(found);
                            }
                        } else {
                            window.set_update_state("current".into());
                            window.set_update_message(
                                format!(
                                    "You have version {}. The latest published release is {}.",
                                    argentum_update::CURRENT_VERSION,
                                    found.version
                                )
                                .into(),
                            );
                        }
                    }
                    Err(error_value) => {
                        window.set_update_state("error".into());
                        window.set_update_message(error_value.to_string().into());
                    }
                });
            });
        });
    }
    {
        let weak = ui.weak_window();
        let handle = runtime.clone();
        let release = release.clone();
        let verified = verified.clone();
        let directory = app_paths.data_dir.join("updates");
        app.on_update_download(move || {
            let Some(found) = release.lock().ok().and_then(|slot| slot.clone()) else {
                return;
            };
            let weak = weak.clone();
            let verified = verified.clone();
            let directory = directory.clone();
            if let Some(window) = weak.upgrade() {
                window.set_update_state("downloading".into());
                window.set_update_progress(0.0);
                window.set_update_message("Downloading.".into());
            }
            handle.spawn(async move {
                let progress_ui = weak.clone();
                let mut last_percent = 0u64;
                let result =
                    argentum_update::download_verified(&found, &directory, |received, total| {
                        let Some(total) = total.filter(|total| *total > 0) else {
                            return;
                        };
                        let percent = received.saturating_mul(100) / total;
                        if percent != last_percent {
                            last_percent = percent;
                            let _ = progress_ui.upgrade_in_event_loop(move |window| {
                                window.set_update_progress(percent as f32 / 100.0);
                                window
                                    .set_update_message(format!("Downloading, {percent}%.").into());
                            });
                        }
                    })
                    .await;
                let _ = weak.upgrade_in_event_loop(move |window| match result {
                    Ok(path) => {
                        if let Ok(mut slot) = verified.lock() {
                            *slot = Some(path);
                        }
                        window.set_update_state("ready".into());
                        window.set_update_message("Downloaded, and the checksum matches.".into());
                    }
                    Err(error_value) => {
                        window.set_update_state("error".into());
                        window.set_update_message(error_value.to_string().into());
                    }
                });
            });
        });
    }
    {
        let weak = ui.weak_window();
        let verified = verified.clone();
        app.on_update_install(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let fail = |message: String| {
                window.set_update_state("error".into());
                window.set_update_message(message.into());
            };
            let Ok(executable) = env::current_exe() else {
                return fail("Argentum could not find its own program file.".to_owned());
            };
            if let Some(reason) = argentum_update::install_blocker(&executable) {
                return fail(reason.to_owned());
            }
            let Some(path) = verified.lock().ok().and_then(|slot| slot.clone()) else {
                return fail("Download the update first.".to_owned());
            };
            window.set_update_state("installing".into());
            if let Err(error_value) = argentum_update::swap_executable(&executable, &path) {
                return fail(error_value.to_string());
            }
            match std::process::Command::new(&executable).spawn() {
                Ok(_) => {
                    let _ = slint::quit_event_loop();
                }
                Err(_) => {
                    let message = match argentum_update::restore_previous(&executable) {
                        Ok(()) => {
                            "The new version would not start, so the previous one was restored."
                                .to_owned()
                        }
                        Err(error_value) => error_value.to_string(),
                    };
                    fail(message);
                }
            }
        });
    }
}
