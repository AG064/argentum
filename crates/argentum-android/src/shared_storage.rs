// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Settings > Workspace on Android: the optional All files access permission,
//! the folder picker, and switching between a chosen folder in Internal
//! storage and the app's own folder.
//!
//! Every chosen path goes through `argentum_platform::validate_shared_workspace`
//! here and again at the next start. The workspace itself is then the ordinary
//! path-based workspace, so the broker, tools, approvals and change records
//! work as they do on desktop.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use android_activity::AndroidApp;
use argentum_app::workspace_choice::{parse_picked, picked_to_path};
use argentum_platform::{validate_shared_workspace, AppPaths, SharedWorkspaceRefusal};
use argentum_ui::{App, UiHandle};
use slint::{ComponentHandle, SharedString};

/// The All files access state, as `ArgentumSharedStorage` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllFilesAccess {
    Unsupported,
    Off,
    On,
}

fn with_activity<T>(
    app: &AndroidApp,
    call: impl FnOnce(&mut jni::Env<'_>, &jni::objects::JObject<'_>) -> jni::errors::Result<T>,
) -> Option<T> {
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| {
        let activity = unsafe {
            env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
        };
        call(env, activity.as_ref())
    })
    .ok()
}

pub(crate) fn all_files_access(app: &AndroidApp) -> AllFilesAccess {
    match with_activity(app, |env, activity| {
        env.call_method(
            activity,
            jni::jni_str!("argentumAllFilesAccess"),
            jni::jni_sig!("()I"),
            &[],
        )?
        .i()
    }) {
        Some(2) => AllFilesAccess::On,
        Some(1) => AllFilesAccess::Off,
        _ => AllFilesAccess::Unsupported,
    }
}

fn call_bool(app: &AndroidApp, method: Method) -> bool {
    with_activity(app, |env, activity| {
        match method {
            Method::OpenSettings => env.call_method(
                activity,
                jni::jni_str!("argentumOpenAllFilesSettings"),
                jni::jni_sig!("()Z"),
                &[],
            ),
            Method::RequestFolder => env.call_method(
                activity,
                jni::jni_str!("argentumRequestFolderPath"),
                jni::jni_sig!("()Z"),
                &[],
            ),
        }?
        .z()
    })
    .unwrap_or(false)
}

#[derive(Clone, Copy)]
enum Method {
    OpenSettings,
    RequestFolder,
}

fn call_string(app: &AndroidApp, take_picked: bool) -> Option<String> {
    with_activity(app, |env, activity| {
        let object = if take_picked {
            env.call_method(
                activity,
                jni::jni_str!("argentumTakePickedFolder"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
        } else {
            env.call_method(
                activity,
                jni::jni_str!("argentumPrimaryStorageRoot"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
        }
        .l()?;
        if object.is_null() {
            return Ok(None);
        }
        let string = env.cast_local::<jni::objects::JString>(object)?;
        Ok(Some(string.try_to_string(env)?))
    })
    .flatten()
}

/// The real path of primary shared storage, such as `/storage/emulated/0`.
pub(crate) fn primary_storage_root(app: &AndroidApp) -> Option<PathBuf> {
    call_string(app, false)
        .filter(|root| root.starts_with('/') && !root.contains('\0'))
        .map(PathBuf::from)
}

/// Starts Argentum again in a new process. Call only after the native host
/// has shut down.
pub(crate) fn restart(app: &AndroidApp) {
    let _ = with_activity(app, |env, activity| {
        env.call_method(
            activity,
            jni::jni_str!("argentumRestart"),
            jni::jni_sig!("()V"),
            &[],
        )?
        .v()
    });
}

/// Connects the Workspace page on Android. `restart` is set when the owner
/// confirmed a different workspace; the host restarts after a clean shutdown.
pub(crate) fn connect(
    ui: &UiHandle,
    app: AndroidApp,
    paths: AppPaths,
    restart: Rc<Cell<bool>>,
    chosen_folder_open: bool,
    startup_notice: Option<String>,
) {
    let window = ui.window();
    let state = window.global::<App>();
    let access = all_files_access(&app);
    state.set_workspace_path_example("/storage/emulated/0/Documents/my-app".into());
    state.set_workspace_is_chosen_folder(chosen_folder_open);
    state.set_workspace_all_files_supported(access != AllFilesAccess::Unsupported);
    state.set_workspace_all_files_granted(access == AllFilesAccess::On);
    state.set_workspace_picker_available(access != AllFilesAccess::Unsupported);
    if let Some(notice) = startup_notice {
        state.set_workspace_notice(notice.clone().into());
        state.set_toast_text(notice.into());
    }

    let weak = ui.weak_window();
    let refresh_app = app.clone();
    state.on_refresh_workspace_access(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<App>();
        let granted = all_files_access(&refresh_app) == AllFilesAccess::On;
        if state.get_workspace_all_files_granted() != granted {
            state.set_workspace_all_files_granted(granted);
        }
        let Some(picked) = call_string(&refresh_app, true) else {
            return;
        };
        let root = primary_storage_root(&refresh_app);
        match picked_to_path(&parse_picked(&picked), root.as_deref()) {
            None => {}
            Some(Ok(path)) => {
                state.set_workspace_draft(path.into());
                state.set_workspace_notice(SharedString::default());
            }
            Some(Err(refusal)) => state.set_workspace_notice(refusal.message().into()),
        }
    });

    let settings_app = app.clone();
    state.on_open_all_files_settings(move || {
        if !call_bool(&settings_app, Method::OpenSettings) {
            tracing::warn!("could not open the All files access settings");
        }
    });

    let weak = ui.weak_window();
    let choose_app = app.clone();
    state.on_choose_workspace_folder(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<App>();
        if all_files_access(&choose_app) != AllFilesAccess::On {
            state.set_workspace_all_files_granted(false);
            state.set_workspace_notice(SharedWorkspaceRefusal::PermissionMissing.message().into());
            return;
        }
        if !call_bool(&choose_app, Method::RequestFolder) {
            state.set_workspace_notice(argentum_app::workspace_choice::PICKER_FAILED.into());
        }
    });

    let weak = ui.weak_window();
    let open_app = app.clone();
    let open_paths = paths.clone();
    let open_restart = restart.clone();
    state.on_open_workspace(move |path| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<App>();
        if all_files_access(&open_app) != AllFilesAccess::On {
            state.set_workspace_notice(SharedWorkspaceRefusal::PermissionMissing.message().into());
            return;
        }
        let Some(root) = primary_storage_root(&open_app) else {
            state.set_workspace_notice(
                SharedWorkspaceRefusal::OutsideSharedStorage
                    .message()
                    .into(),
            );
            return;
        };
        let checked = validate_shared_workspace(Path::new(path.trim()), &root)
            .and_then(|workspace| open_paths.save_shared_workspace_choice(&workspace));
        match checked {
            Ok(()) => {
                open_restart.set(true);
                let _ = slint::quit_event_loop();
            }
            Err(refusal) => state.set_workspace_notice(refusal.message().into()),
        }
    });

    let weak = ui.weak_window();
    state.on_use_app_workspace(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if paths.clear_shared_workspace_choice() {
            restart.set(true);
            let _ = slint::quit_event_loop();
        } else {
            window
                .global::<App>()
                .set_workspace_notice("Argentum could not forget the chosen folder.".into());
        }
    });
}
