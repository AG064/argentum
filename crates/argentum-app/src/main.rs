use std::env;
use std::path::{Path, PathBuf};

use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{AppCommand, AppEvent, ProviderKind, ProviderProfile};
use argentum_ui::{connect_commands, UiHandle, WeakUiHandle};
use slint::ComponentHandle;
use tokio::runtime::Builder;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod legacy_compat;

fn main() {
    if let Err(error_value) = run() {
        let message = safe_startup_error(error_value.as_ref());
        eprintln!("{message}");
        show_startup_error(&message);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("SLINT_BACKEND").is_none() {
        env::set_var("SLINT_BACKEND", "femtovg");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("argentum=info")),
        )
        .with_target(false)
        .try_init()
        .ok();

    let compatibility = legacy_compat::discover();
    let explicit_workspace = env::var_os("ARGENTUM_WORKSPACE").map(PathBuf::from);
    let (workspace_root, uses_legacy_workspace) = select_workspace(
        explicit_workspace,
        compatibility.workspace.as_deref(),
        env::current_dir()?,
    );
    let environment_has_minimax = has_nonempty_env("MINIMAX_API_KEY");
    let legacy_minimax_key = if uses_legacy_workspace && !environment_has_minimax {
        compatibility.minimax_key
    } else {
        None
    };
    let should_seed_minimax = environment_has_minimax || legacy_minimax_key.is_some();
    let mut host_config = HostConfig::discover(workspace_root)?;
    if let Some(secret) = legacy_minimax_key {
        host_config = host_config.with_provider_credential("minimax", secret)?;
    }
    let host = CommandHost::start(host_config)?;
    let async_runtime = Builder::new_multi_thread().enable_all().build()?;
    if should_seed_minimax
        && !host
            .provider_profiles()?
            .iter()
            .any(|profile| profile.id == "minimax")
    {
        async_runtime.block_on(host.client().dispatch(AppCommand::SaveProviderProfile {
            profile: legacy_minimax_profile(),
        }))?;
    }
    let ui = UiHandle::new()?;
    if let Some(size) = env::var_os("ARGENTUM_UI_SIZE")
        .and_then(|value| value.into_string().ok())
        .and_then(|value| parse_window_size(&value))
    {
        ui.window().window().set_size(size);
    }
    ui.window()
        .set_workspace_label(display_workspace(host.workspace_root()).into());
    let ui_for_events = ui.weak_handle();
    let mut events = host.subscribe();
    let host_for_resync = host.clone();

    async_runtime.spawn(async move {
        let mut pending_event = None;
        loop {
            let received = match pending_event.take() {
                Some(event) => Ok(event),
                None => events.recv().await,
            };
            match received {
                Ok(
                    mut event @ (AppEvent::AssistantDelta { .. }
                    | AppEvent::AssistantReasoningDelta { .. }),
                ) => {
                    let mut lagged = None;
                    loop {
                        match events.try_recv() {
                            Ok(next_event) => {
                                if let Some(next_event) =
                                    append_matching_stream_delta(&mut event, next_event)
                                {
                                    pending_event = Some(next_event);
                                    break;
                                }
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Lagged(skipped)) => {
                                lagged = Some(skipped);
                                break;
                            }
                            Err(TryRecvError::Closed) => break,
                        }
                    }
                    if !deliver_ui_event(ui_for_events.clone(), event) {
                        break;
                    }
                    if let Some(skipped) = lagged {
                        warn!(
                            skipped,
                            "application event receiver lagged while coalescing output"
                        );
                        republish_after_lag(&host_for_resync, &ui_for_events);
                    }
                }
                Ok(event) => {
                    if !deliver_ui_event(ui_for_events.clone(), event) {
                        break;
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    warn!(
                        skipped,
                        "application event receiver lagged; republishing current state"
                    );
                    republish_after_lag(&host_for_resync, &ui_for_events);
                }
                Err(RecvError::Closed) => {
                    warn!("application event stream closed");
                    break;
                }
            }
        }
    });

    let command_runtime = async_runtime.handle().clone();
    let command_client = host.client();
    let ui_for_command_errors = ui.weak_handle();
    connect_commands(ui.window(), move |command| {
        let client = command_client.clone();
        let ui = ui_for_command_errors.clone();
        command_runtime.spawn(async move {
            if let Err(error_value) = client.dispatch(command.clone()).await {
                error!(error = %error_value, "application command failed");
                let _ = deliver_ui_command_failure(ui, command, error_value.to_string());
            }
        });
    });

    host.publish_initial_state()?;
    info!(workspace = %host.workspace_root().display(), "Argentum native shell starting");
    ui.show()?;
    slint::run_event_loop()?;
    ui.hide()?;
    Ok(())
}

fn append_matching_stream_delta(current: &mut AppEvent, next: AppEvent) -> Option<AppEvent> {
    match (current, next) {
        (
            AppEvent::AssistantDelta { run_id, text },
            AppEvent::AssistantDelta {
                run_id: next_run_id,
                text: next_text,
            },
        ) if *run_id == next_run_id => {
            text.push_str(&next_text);
            None
        }
        (
            AppEvent::AssistantReasoningDelta { run_id, text },
            AppEvent::AssistantReasoningDelta {
                run_id: next_run_id,
                text: next_text,
            },
        ) if *run_id == next_run_id => {
            text.push_str(&next_text);
            None
        }
        (_, next) => Some(next),
    }
}

fn republish_after_lag(host: &CommandHost, ui: &WeakUiHandle) {
    if let Err(error_value) = host.publish_initial_state() {
        error!(error = %error_value, "unable to republish application state");
        let _ = deliver_ui_event(
            ui.clone(),
            AppEvent::Error {
                message: "Argentum could not refresh the current workspace state.".into(),
                recoverable: true,
            },
        );
    }
}

fn deliver_ui_event(ui: WeakUiHandle, event: AppEvent) -> bool {
    match slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.apply_event(&event);
        }
    }) {
        Ok(()) => true,
        Err(error_value) => {
            error!(error = %error_value, "unable to deliver application event to UI");
            false
        }
    }
}

fn deliver_ui_command_failure(ui: WeakUiHandle, command: AppCommand, message: String) -> bool {
    match slint::invoke_from_event_loop(move || {
        if let Some(handle) = ui.upgrade() {
            handle.apply_command_failure(&command, &message);
        }
    }) {
        Ok(()) => true,
        Err(error_value) => {
            error!(error = %error_value, "unable to deliver command failure to UI");
            false
        }
    }
}

fn safe_startup_error(_error_value: &(dyn std::error::Error + 'static)) -> String {
    "Argentum could not start. Check workspace access and provider configuration, then try again."
        .into()
}

#[cfg(windows)]
fn show_startup_error(message: &str) {
    use std::os::windows::ffi::OsStrExt;

    const MB_ICONERROR: u32 = 0x0000_0010;
    const MB_OK: u32 = 0x0000_0000;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut core::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }

    let text = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let caption = std::ffi::OsStr::new("Argentum startup error")
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // The pointers remain valid for the duration of this blocking native call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn show_startup_error(_message: &str) {}

fn parse_window_size(value: &str) -> Option<slint::LogicalSize> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let width = width.trim().parse::<u32>().ok()?;
    let height = height.trim().parse::<u32>().ok()?;

    if !(320..=3840).contains(&width) || !(320..=2160).contains(&height) {
        return None;
    }

    Some(slint::LogicalSize::new(width as f32, height as f32))
}

fn display_workspace(path: &Path) -> String {
    let raw = path.display().to_string();
    if let Some(rest) = raw.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = raw.strip_prefix("\\\\?\\") {
        rest.to_owned()
    } else {
        raw
    }
}

fn select_workspace(
    explicit: Option<PathBuf>,
    legacy: Option<&Path>,
    fallback: PathBuf,
) -> (PathBuf, bool) {
    if let Some(path) = explicit {
        return (path, false);
    }
    if let Some(path) = legacy {
        return (path.to_path_buf(), true);
    }
    (fallback, false)
}

fn has_nonempty_env(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn legacy_minimax_profile() -> ProviderProfile {
    ProviderProfile {
        id: "minimax".into(),
        label: "MiniMax".into(),
        kind: ProviderKind::OpenAiCompatible,
        endpoint: "https://api.minimax.io/v1/".into(),
        model: "MiniMax-M2.7".into(),
        selected: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_matching_stream_delta, display_workspace, legacy_minimax_profile, parse_window_size,
        select_workspace,
    };
    use argentum_domain::AppEvent;
    use std::path::{Path, PathBuf};

    #[test]
    fn display_workspace_removes_windows_extended_path_prefixes() {
        assert_eq!(
            display_workspace(Path::new(r"\\?\A:\ag064\argentum")),
            r"A:\ag064\argentum"
        );
        assert_eq!(
            display_workspace(Path::new(r"\\?\UNC\server\share")),
            r"\\server\share"
        );
    }

    #[test]
    fn preview_window_size_is_bounded_and_explicit() {
        let size = parse_window_size("430x800").expect("valid phone fixture");
        assert_eq!(size.width, 430.0);
        assert_eq!(size.height, 800.0);

        assert!(parse_window_size("800X360").is_some());
        assert!(parse_window_size("319x800").is_none());
        assert!(parse_window_size("430x2161").is_none());
        assert!(parse_window_size("430 by 800").is_none());
    }

    #[test]
    fn legacy_minimax_profile_uses_the_current_openai_compatible_route() {
        let profile = legacy_minimax_profile();

        assert_eq!(profile.id, "minimax");
        assert_eq!(profile.endpoint, "https://api.minimax.io/v1/");
        assert_eq!(profile.model, "MiniMax-M2.7");
        assert!(!profile.selected);
    }

    #[test]
    fn explicit_workspace_never_inherits_legacy_workspace_credentials() {
        let (workspace, uses_legacy_credentials) = select_workspace(
            Some(PathBuf::from(r"A:\explicit")),
            Some(Path::new(r"A:\legacy")),
            PathBuf::from(r"A:\fallback"),
        );

        assert_eq!(workspace, PathBuf::from(r"A:\explicit"));
        assert!(!uses_legacy_credentials);
    }

    #[test]
    fn legacy_workspace_is_used_only_without_an_explicit_override() {
        let (workspace, uses_legacy_credentials) = select_workspace(
            None,
            Some(Path::new(r"A:\legacy")),
            PathBuf::from(r"A:\fallback"),
        );

        assert_eq!(workspace, PathBuf::from(r"A:\legacy"));
        assert!(uses_legacy_credentials);
    }

    #[test]
    fn consecutive_answer_and_reasoning_chunks_coalesce_by_kind_and_run() {
        let run_id = "01234567-89ab-cdef-0123-456789abcdef"
            .parse()
            .expect("run id");
        let mut answer = AppEvent::AssistantDelta {
            run_id,
            text: "first".into(),
        };
        assert!(append_matching_stream_delta(
            &mut answer,
            AppEvent::AssistantDelta {
                run_id,
                text: " second".into(),
            },
        )
        .is_none());
        assert!(matches!(
            answer,
            AppEvent::AssistantDelta { text, .. } if text == "first second"
        ));

        let mut reasoning = AppEvent::AssistantReasoningDelta {
            run_id,
            text: "step".into(),
        };
        assert!(append_matching_stream_delta(
            &mut reasoning,
            AppEvent::AssistantReasoningDelta {
                run_id,
                text: " two".into(),
            },
        )
        .is_none());
        assert!(matches!(
            reasoning,
            AppEvent::AssistantReasoningDelta { text, .. } if text == "step two"
        ));
    }

    #[test]
    fn stream_coalescing_preserves_kind_and_run_boundaries() {
        let run_id = "01234567-89ab-cdef-0123-456789abcdef"
            .parse()
            .expect("run id");
        let other_run_id = "fedcba98-7654-3210-fedc-ba9876543210"
            .parse()
            .expect("other run id");
        let mut reasoning = AppEvent::AssistantReasoningDelta {
            run_id,
            text: "step".into(),
        };

        let different_kind = append_matching_stream_delta(
            &mut reasoning,
            AppEvent::AssistantDelta {
                run_id,
                text: "answer".into(),
            },
        )
        .expect("different event kind");
        assert!(matches!(different_kind, AppEvent::AssistantDelta { .. }));

        let different_run = append_matching_stream_delta(
            &mut reasoning,
            AppEvent::AssistantReasoningDelta {
                run_id: other_run_id,
                text: "other".into(),
            },
        )
        .expect("different run");
        assert!(matches!(
            different_run,
            AppEvent::AssistantReasoningDelta { run_id, .. } if run_id == other_run_id
        ));
    }
}
