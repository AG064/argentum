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

#[derive(Debug)]
enum AndroidHostError {
    Platform,
    Slint,
    Host,
}

impl fmt::Display for AndroidHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Argentum could not start its Android host.")
    }
}

impl std::error::Error for AndroidHostError {}

#[unsafe(no_mangle)]
pub fn android_main(app: AndroidApp) {
    if let Err(error) = run_android(app) {
        eprintln!("{error}");
    }
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
    .map_err(|_| AndroidHostError::Slint)?;

    let validated = android_app_private_workspace_for_app(&app).map_err(|error| {
        tracing::error!(stage = "private-workspace", error = ?error, "Android host platform initialization failed");
        AndroidHostError::Platform
    })?;
    let secret_store = Arc::new(AndroidSecretStore::new(app.clone()));
    let config = HostConfig::discover_with_validated_app_paths(validated, secret_store)
        .map_err(|_| AndroidHostError::Host)?;

    let configure_app = app.clone();
    let configure_tracker = tracker.clone();
    let configure_pending = pending_insets.clone();
    let configure_ui_slot = ui_slot.clone();
    let configure_registration = close_registration.clone();
    let inference_foreground = Arc::new(AndroidInferenceForeground::new(app.clone()));
    let result = run_native_host(
        config,
        NativeHostOptions {
            workspace_label: "App workspace".into(),
            seed_profiles: android_provider_profiles().into(),
            inference_foreground: Some(inference_foreground),
        },
        move |ui| {
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
    result.map_err(|error| {
        tracing::error!(stage = "native-host", error = ?error, "Android native host execution failed");
        AndroidHostError::Host
    })
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
        .map_err(|_| AndroidHostError::Platform)?;
    u64::try_from(generation)
        .ok()
        .filter(|generation| *generation != 0)
        .ok_or(AndroidHostError::Platform)
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
        let array = JIntArray::cast_local(env, object)?;
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
