// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use android_activity::{AndroidApp, MainEvent, PollEvent};
use argentum_app::{run_native_host, NativeHostOptions, NativeInferenceForeground};
use argentum_cli::HostConfig;
use argentum_domain::{ProviderKind, ProviderProfile};
use argentum_platform::{android_app_private_workspace_for_app, AndroidSecretStore};
use argentum_ui::{UiHandle, WeakUiHandle};

use crate::insets::{
    AndroidInsets, AndroidInsetsTracker, AndroidWindowMetrics, DEFAULT_DENSITY_DPI,
};
use crate::jni_bridge::{register_close_action, CloseActionRegistration};

/// Why the host could not start. The stage and detail reach logcat through
/// stderr, so a failed start can be diagnosed without a debugger. Details are
/// error descriptions and app-private paths, never secrets.
#[derive(Debug)]
struct AndroidHostError {
    stage: &'static str,
    detail: String,
    /// What the owner is shown. Plain words only, never the detail.
    message: String,
}

const GENERIC_STARTUP_MESSAGE: &str =
    "Argentum could not prepare its app storage. Restart the phone, then open Argentum again.";

impl AndroidHostError {
    fn new(stage: &'static str, detail: impl fmt::Debug) -> Self {
        Self {
            stage,
            detail: format!("{detail:?}"),
            message: GENERIC_STARTUP_MESSAGE.into(),
        }
    }
}

impl fmt::Display for AndroidHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Argentum could not start its Android host at {}: {}",
            self.stage, self.detail
        )
    }
}

impl std::error::Error for AndroidHostError {}

#[unsafe(no_mangle)]
pub fn android_main(app: AndroidApp) {
    // A panic on a phone otherwise leaves only its message in logcat. The
    // backtrace has names when the library keeps its symbols
    // (-PkeepNativeSymbols=true).
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!("argentum panicked: {info}\n{backtrace}");
    }));
    if let Err(error) = run_android(app) {
        eprintln!("{error}");
        // Without a window the activity would just close. The interface
        // toolkit is up for every stage after its own, so say why instead.
        if error.stage != "slint" {
            show_startup_error(&error.message);
        }
    }
}

fn show_startup_error(message: &str) {
    use slint::ComponentHandle;

    let Ok(window) = argentum_ui::StartupErrorWindow::new() else {
        return;
    };
    window.set_message(message.into());
    // Room for the status bar and gesture bar, which this window does not measure.
    window.set_inset_top(48.0);
    window.set_inset_bottom(32.0);
    window.on_dismiss(|| {
        let _ = slint::quit_event_loop();
    });
    let _ = window.run();
}

fn run_android(app: AndroidApp) -> Result<(), AndroidHostError> {
    let activity_generation = read_activity_generation(&app)?;
    let tracker = Rc::new(RefCell::new(AndroidInsetsTracker::default()));
    let pending_insets = Rc::new(RefCell::new(None::<AndroidInsets>));
    let ui_slot = Rc::new(RefCell::new(None::<WeakUiHandle>));
    let close_registration = Rc::new(RefCell::new(None::<CloseActionRegistration>));

    let listener_app = app.clone();
    let listener_tracker = tracker.clone();
    let listener_pending = pending_insets.clone();
    let listener_ui = ui_slot.clone();
    slint::android::init_with_event_listener(app.clone(), move |event| {
        if matches!(event, PollEvent::Main(MainEvent::TerminateWindow { .. })) {
            listener_tracker.borrow_mut().reset();
            *listener_pending.borrow_mut() = None;
            return;
        }
        if should_refresh_insets(event) {
            refresh_android_insets(
                &listener_app,
                &listener_tracker,
                &listener_pending,
                &listener_ui,
            );
        }
    })
    .map_err(|error| AndroidHostError::new("slint", error))?;

    let validated = android_app_private_workspace_for_app(&app).map_err(|error| {
        tracing::error!(stage = "private-workspace", error = ?error, "Android host platform initialization failed");
        AndroidHostError::new("private-workspace", error)
    })?;
    // The app-private workspace is the default. A folder the owner chose in
    // Internal storage replaces it only while All files access is on and the
    // folder still passes the shared-storage check.
    let all_files_on =
        crate::shared_storage::all_files_access(&app) == crate::shared_storage::AllFilesAccess::On;
    let primary_root = crate::shared_storage::primary_storage_root(&app);
    let app_paths = validated.app_paths().clone();
    let (validated, chosen_folder_open, startup_notice) =
        match argentum_platform::shared_workspace_at_start(
            &app_paths,
            all_files_on,
            primary_root.as_deref(),
        ) {
            argentum_platform::SharedWorkspaceStart::AppPrivate => (validated, false, None),
            argentum_platform::SharedWorkspaceStart::Shared(shared) => {
                (validated.with_shared_workspace(shared), true, None)
            }
            argentum_platform::SharedWorkspaceStart::Fallback { notice, .. } => {
                (validated, false, Some(notice))
            }
        };
    let secret_store = Arc::new(AndroidSecretStore::new(app.clone()));
    let config = HostConfig::discover_with_validated_app_paths(validated, secret_store).map_err(
        |error| {
            forget_failed_choice(&app_paths, chosen_folder_open);
            AndroidHostError::new("host-config", error)
        },
    )?;
    let workspace_label = if chosen_folder_open {
        argentum_ui::display_workspace_label(&config.workspace)
    } else {
        "App workspace".to_owned()
    };
    let restart_requested = Rc::new(std::cell::Cell::new(false));
    reconcile_abandoned_owner(&config)?;

    let configure_app = app.clone();
    let configure_tracker = tracker.clone();
    let configure_pending = pending_insets.clone();
    let configure_ui_slot = ui_slot.clone();
    let configure_registration = close_registration.clone();
    let configure_paths = app_paths.clone();
    let configure_restart = restart_requested.clone();
    let mut configure_notice = startup_notice;
    let inference_foreground = Arc::new(AndroidInferenceForeground::new(app.clone()));
    let insets_timer = slint::Timer::default();
    let insets_timer_ref = &insets_timer;
    let result = run_native_host(
        config,
        NativeHostOptions {
            workspace_label,
            seed_profiles: android_provider_profiles().into(),
            inference_foreground: Some(inference_foreground),
            session_export: Some(share_session_export_sink(app.clone())),
        },
        move |ui| {
            ui.window().set_touch_device(true);
            // Android installs updates from a new APK, not from inside the app.
            use slint::ComponentHandle;
            let app = ui.window().global::<argentum_ui::App>();
            app.set_update_current(env!("CARGO_PKG_VERSION").into());
            app.set_update_in_app(false);
            let report_app = configure_app.clone();
            app.on_report_problem(move || open_issue_page(&report_app));
            let copy_app = configure_app.clone();
            app.on_copy_text(move |text| copy_text(&copy_app, text.as_str()));
            crate::shared_storage::connect(
                ui,
                configure_app.clone(),
                configure_paths.clone(),
                configure_restart.clone(),
                chosen_folder_open,
                configure_notice.take(),
            );
            app.set_update_message(
                "To update, download the latest APK from the Argentum releases page on GitHub."
                    .into(),
            );
            *configure_ui_slot.borrow_mut() = Some(ui.weak_handle());
            if let Some(insets) = configure_pending.borrow_mut().take() {
                apply_android_insets(ui, insets);
            } else {
                refresh_android_insets(
                    &configure_app,
                    &configure_tracker,
                    &configure_pending,
                    &configure_ui_slot,
                );
            }
            let timer_app = configure_app.clone();
            let timer_tracker = configure_tracker.clone();
            let timer_pending = configure_pending.clone();
            let timer_ui_slot = configure_ui_slot.clone();
            // NativeActivity reports no event when the on-screen keyboard opens
            // or closes, so the insets are read again on the UI thread. The read
            // is one JNI call, and the layout changes only when a value does.
            insets_timer_ref.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(150),
                move || {
                    refresh_android_insets(
                        &timer_app,
                        &timer_tracker,
                        &timer_pending,
                        &timer_ui_slot,
                    );
                },
            );
            *configure_registration.borrow_mut() =
                Some(register_close_action(activity_generation, || {
                    slint::invoke_from_event_loop(|| {
                        let _ = slint::quit_event_loop();
                    })
                    .is_ok()
                }));
        },
    );
    close_registration.borrow_mut().take();
    match result {
        Ok(()) => {
            if restart_requested.get() {
                // The host has shut down and closed the database, so a new
                // process can open the chosen workspace.
                crate::shared_storage::restart(&app);
            }
            Ok(())
        }
        Err(error) => {
            tracing::error!(stage = "native-host", error = ?error, "Android native host execution failed");
            forget_failed_choice(&app_paths, chosen_folder_open);
            let mut message = argentum_app::startup_error_message(&error);
            if chosen_folder_open {
                message.push_str(" Argentum will open its own folder next time.");
            }
            Err(AndroidHostError {
                message,
                ..AndroidHostError::new("native-host", error)
            })
        }
    }
}

/// A chosen folder that stopped the host from starting is forgotten, so the
/// next start opens the app-private workspace instead of failing again.
fn forget_failed_choice(paths: &argentum_platform::AppPaths, chosen_folder_open: bool) {
    if chosen_folder_open && !paths.clear_shared_workspace_choice() {
        eprintln!("argentum could not forget the chosen workspace folder");
    }
}

struct AndroidInferenceForeground {
    app: AndroidApp,
    active_generation: AtomicU64,
}

enum InferenceForegroundAction {
    Start,
    Stop,
}

impl AndroidInferenceForeground {
    fn new(app: AndroidApp) -> Self {
        Self {
            app,
            active_generation: AtomicU64::new(0),
        }
    }

    fn call_activity(&self, action: InferenceForegroundAction, generation: u64) -> bool {
        use jni::objects::JValue;

        let Ok(generation) = i64::try_from(generation) else {
            return false;
        };
        let vm = unsafe { jni::JavaVM::from_raw(self.app.vm_as_ptr().cast()) };
        let raw_activity = self.app.activity_as_ptr() as jni::sys::jobject;
        vm.attach_current_thread(|env| {
            let activity = unsafe {
                env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
            };
            match action {
                InferenceForegroundAction::Start => env.call_method(
                    activity.as_ref(),
                    jni::jni_str!("argentumStartInferenceForeground"),
                    jni::jni_sig!("(J)Z"),
                    &[JValue::Long(generation)],
                ),
                InferenceForegroundAction::Stop => env.call_method(
                    activity.as_ref(),
                    jni::jni_str!("argentumStopInferenceForeground"),
                    jni::jni_sig!("(J)Z"),
                    &[JValue::Long(generation)],
                ),
            }?
            .z()
        })
        .unwrap_or(false)
    }
}

impl NativeInferenceForeground for AndroidInferenceForeground {
    fn start_generation(&self, generation: u64) -> bool {
        if generation == 0 {
            return false;
        }
        let current = self.active_generation.load(Ordering::Acquire);
        if current != 0 && current != generation {
            return false;
        }
        if !self.call_activity(InferenceForegroundAction::Start, generation) {
            return false;
        }
        current == generation
            || self
                .active_generation
                .compare_exchange(0, generation, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    fn stop_generation(&self, generation: u64) -> bool {
        if generation == 0 {
            return false;
        }
        let active = self.active_generation.load(Ordering::Acquire);
        // An unloaded worker may never have started a foreground service.
        // Retrying its disposal is complete when no generation is active.
        if active == 0 {
            return true;
        }
        if active != generation {
            return false;
        }
        if !self.call_activity(InferenceForegroundAction::Stop, generation) {
            return false;
        }
        self.active_generation
            .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn stop_active(&self) -> bool {
        let generation = self.active_generation.load(Ordering::Acquire);
        generation == 0 || self.stop_generation(generation)
    }
}

/// Android ends app processes as a matter of routine, including when the owner
/// swipes the app away, and it has no console for the recovery command. The
/// trusted store reconciliation runs here instead. It acquires the lifetime
/// lock first, so it only proceeds once the previous owner is proven gone, and
/// a live second instance fails closed. It records interrupted work, retires
/// approvals, pauses goals, and replays nothing.
fn reconcile_abandoned_owner(config: &HostConfig) -> Result<(), AndroidHostError> {
    let Some(database) = config.database.as_ref().filter(|path| path.exists()) else {
        return Ok(());
    };
    let settlement = argentum_store::Store::reconcile_recovery(database)
        .map_err(|error| AndroidHostError::new("recovery", error))?;
    if settlement.mutation_performed {
        eprintln!(
            "argentum recovered after an unclean exit: {} runs, {} goal invocations interrupted{}",
            settlement.recovered_run_ids.len(),
            settlement.recovered_invocation_ids.len(),
            if settlement.incomplete {
                ", more remain"
            } else {
                ""
            }
        );
    }
    acknowledge_effect_free_interruptions(database)
}

/// A reconciled run blocks its session until the owner reviews it, which on
/// desktop is `argentum recovery review`. A phone has no console, and the app
/// is ended routinely, so a chat cut off mid-reply would block its session for
/// good. Runs that admitted no external effect are acknowledged here, such as
/// a reply in progress, whose only unknown is the model request's outcome and
/// usage. Any run that may have changed something keeps its hold.
fn acknowledge_effect_free_interruptions(
    database: &std::path::Path,
) -> Result<(), AndroidHostError> {
    use argentum_domain::recovery::{RecoveryEffectState, RecoveryExecutionState};

    let inspection = argentum_store::Store::inspect_recovery(database)
        .map_err(|error| AndroidHostError::new("recovery-inspect", error))?;
    let mut acknowledged = 0_usize;
    for run in inspection.runs.iter().filter(|run| {
        run.execution_state == RecoveryExecutionState::Interrupted
            && run.admission_blocked
            && run.owner_reviewed_at.is_none()
            && run.effect_state != RecoveryEffectState::Admitted
            && run
                .unresolved_effects
                .iter()
                .all(|reason| reason == "provider_request_outcome_unknown")
    }) {
        let Some(revision) = run.recovery_revision else {
            continue;
        };
        argentum_store::Store::review_recovery(database, run.run_id, revision)
            .map_err(|error| AndroidHostError::new("recovery-review", error))?;
        acknowledged += 1;
    }
    if acknowledged > 0 {
        eprintln!("argentum acknowledged {acknowledged} interrupted runs with no external effects");
    }
    Ok(())
}

fn read_activity_generation(app: &AndroidApp) -> Result<u64, AndroidHostError> {
    use jni::{jni_sig, jni_str};

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    let generation = vm
        .attach_current_thread(|env| {
            let activity = unsafe {
                env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
            };
            env.call_method(
                activity.as_ref(),
                jni_str!("argentumActivityGeneration"),
                jni_sig!("()J"),
                &[],
            )?
            .j()
        })
        .map_err(|error| AndroidHostError::new("activity-generation", error))?;
    u64::try_from(generation)
        .ok()
        .filter(|generation| *generation != 0)
        .ok_or_else(|| AndroidHostError::new("activity-generation", "missing"))
}

fn should_refresh_insets(event: &PollEvent<'_>) -> bool {
    matches!(
        event,
        PollEvent::Main(
            MainEvent::InitWindow { .. }
                | MainEvent::WindowResized { .. }
                | MainEvent::ContentRectChanged { .. }
                | MainEvent::InsetsChanged { .. }
                | MainEvent::ConfigChanged { .. }
        )
    )
}

fn refresh_android_insets(
    app: &AndroidApp,
    tracker: &Rc<RefCell<AndroidInsetsTracker>>,
    pending: &Rc<RefCell<Option<AndroidInsets>>>,
    ui_slot: &Rc<RefCell<Option<WeakUiHandle>>>,
) {
    let Some(metrics) = read_window_metrics(app) else {
        return;
    };
    let Some(insets) = tracker.borrow_mut().update(metrics) else {
        return;
    };
    let weak_ui = ui_slot.borrow().clone();
    if let Some(ui) = weak_ui.and_then(|weak| weak.upgrade()) {
        apply_android_insets(&ui, insets);
    } else {
        *pending.borrow_mut() = Some(insets);
    }
}

fn read_window_metrics(app: &AndroidApp) -> Option<AndroidWindowMetrics> {
    let window = app.native_window()?;
    let content = app.content_rect();
    let explicit_insets = read_window_insets(app);
    Some(AndroidWindowMetrics {
        window_width_px: window.width(),
        window_height_px: window.height(),
        content_top_px: content.top,
        content_bottom_px: content.bottom,
        system_bar_top_px: explicit_insets.map(|insets| insets[0]),
        system_bar_bottom_px: explicit_insets.map(|insets| insets[1]),
        ime_bottom_px: explicit_insets.map(|insets| insets[2]),
        density_dpi: app.config().density().unwrap_or(DEFAULT_DENSITY_DPI),
    })
}

fn read_window_insets(app: &AndroidApp) -> Option<[i32; 3]> {
    use jni::objects::JIntArray;
    use jni::{jni_sig, jni_str};

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| -> jni::errors::Result<Option<[i32; 3]>> {
        let activity = unsafe {
            env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
        };
        let object = env
            .call_method(
                activity.as_ref(),
                jni_str!("argentumWindowInsets"),
                jni_sig!("()[I"),
                &[],
            )?
            .l()?;
        if object.is_null() {
            return Ok(None);
        }
        // The Java method is declared to return int[], so the type check that
        // cast_local performs, which looks the class up on every call, is not
        // needed on this path that runs several times a second.
        // SAFETY: a non-null local reference of type int[] from this frame,
        // owned only here.
        let array = unsafe { JIntArray::from_raw(env, object.into_raw()) };
        if array.len(env)? != 3 {
            return Ok(None);
        }
        let mut values = [0; 3];
        array.get_region(env, 0, &mut values)?;
        Ok(values.iter().all(|value| *value >= 0).then_some(values))
    })
    .ok()
    .flatten()
}

fn apply_android_insets(ui: &UiHandle, insets: AndroidInsets) {
    ui.window().set_safe_area_top(insets.safe_top_dp);
    ui.window().set_safe_area_bottom(insets.safe_bottom_dp);
    ui.window().set_keyboard_inset(insets.keyboard_dp);
}

fn android_provider_profiles() -> [ProviderProfile; 2] {
    [
        ProviderProfile {
            id: "deepseek".into(),
            label: "DeepSeek".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "https://api.deepseek.com/".into(),
            model: "deepseek-flash".into(),
            selected: false,
        },
        ProviderProfile {
            id: "minimax".into(),
            label: "MiniMax".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "https://api.minimax.io/v1/".into(),
            model: "MiniMax-M2.7".into(),
            selected: false,
        },
    ]
}

/// Asks the activity to open the issue page in a browser.
fn open_issue_page(app: &AndroidApp) {
    use jni::objects::JValue;

    let url = argentum_app::issue_report_url("android");
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    let opened = vm
        .attach_current_thread(|env| {
            let activity = unsafe {
                env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
            };
            let java_url = env.new_string(&url)?;
            env.call_method(
                activity.as_ref(),
                jni::jni_str!("argentumOpenIssuePage"),
                jni::jni_sig!("(Ljava/lang/String;)Z"),
                &[JValue::Object(java_url.as_ref())],
            )?
            .z()
        })
        .unwrap_or(false);
    if !opened {
        tracing::warn!("could not open the issue page");
    }
}

/// Copies text with Android's clipboard. Slint's own clipboard does not reach
/// it from NativeActivity, so copy buttons did nothing on the phone.
fn copy_text(app: &AndroidApp, text: &str) -> bool {
    use jni::objects::JValue;

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| {
        let activity = unsafe {
            env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
        };
        let java_text = env.new_string(text)?;
        env.call_method(
            activity.as_ref(),
            jni::jni_str!("argentumCopyText"),
            jni::jni_sig!("(Ljava/lang/String;)Z"),
            &[JValue::Object(java_text.as_ref())],
        )?
        .z()
    })
    .unwrap_or(false)
}

/// Session exports go to the Android share sheet, where the owner picks where
/// they go: Files, a mail app, or anything else that takes a file.
fn share_session_export_sink(app: AndroidApp) -> argentum_app::session_export::SessionExportSink {
    use argentum_app::session_export::ExportDelivery;

    Box::new(move |export| {
        if share_session_export(&app, &export.file_name, &export.json) {
            Ok(ExportDelivery::Shared)
        } else {
            Err("Argentum could not open the share sheet for the export.".to_owned())
        }
    })
}

fn share_session_export(app: &AndroidApp, name: &str, json: &str) -> bool {
    use jni::objects::JValue;

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| {
        let activity = unsafe {
            env.as_cast_raw::<jni::refs::Global<jni::objects::JObject<'_>>>(&raw_activity)?
        };
        let java_name = env.new_string(name)?;
        let java_json = env.new_string(json)?;
        env.call_method(
            activity.as_ref(),
            jni::jni_str!("argentumShareSessionExport"),
            jni::jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Z"),
            &[
                JValue::Object(java_name.as_ref()),
                JValue::Object(java_json.as_ref()),
            ],
        )?
        .z()
    })
    .unwrap_or(false)
}
