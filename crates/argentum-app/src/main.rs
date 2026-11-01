use std::env;
use std::path::{Path, PathBuf};

use argentum_app::{run_native_host, NativeHostError, NativeHostOptions};
use argentum_cli::{HostConfig, HostError};
use argentum_domain::{ProviderKind, ProviderProfile};
use argentum_platform::{AppPaths, PlatformError};
use argentum_ui::display_workspace_label;
use slint::ComponentHandle;

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
    #[cfg(windows)]
    if env::var_os("SLINT_BACKEND").is_none() {
        env::set_var("SLINT_BACKEND", "femtovg");
    }

    let compatibility = legacy_compat::discover();
    let explicit_workspace = env::var_os("ARGENTUM_WORKSPACE").map(PathBuf::from);
    let app_paths = AppPaths::discover()?;
    let persisted_workspace = app_paths.load_workspace()?;
    let (selected_workspace, source) = select_workspace(
        explicit_workspace,
        persisted_workspace.as_deref(),
        compatibility.workspace.as_deref(),
        env::current_dir()?,
    );
    let workspace_root = match source {
        WorkspaceSource::Explicit | WorkspaceSource::Persisted => {
            canonical_workspace(&selected_workspace)?
        }
        WorkspaceSource::Legacy | WorkspaceSource::CurrentDirectory => {
            app_paths.save_workspace(&selected_workspace)?
        }
    };
    let uses_legacy_workspace = source == WorkspaceSource::Legacy;
    let environment_has_minimax = has_nonempty_env("MINIMAX_API_KEY");
    let legacy_minimax_key = if uses_legacy_workspace && !environment_has_minimax {
        compatibility.minimax_key
    } else {
        None
    };
    let should_seed_minimax = environment_has_minimax || legacy_minimax_key.is_some();
    let mut host_config = HostConfig::discover(&workspace_root)?;
    if let Some(secret) = legacy_minimax_key {
        host_config = host_config.with_provider_credential("minimax", secret)?;
    }
    let preview_size = env::var_os("ARGENTUM_UI_SIZE")
        .and_then(|value| value.into_string().ok())
        .and_then(|value| parse_window_size(&value));
    let seed_profiles = should_seed_minimax
        .then(legacy_minimax_profile)
        .into_iter()
        .collect();
    run_native_host(
        host_config,
        NativeHostOptions {
            workspace_label: display_workspace_label(&workspace_root),
            seed_profiles,
            inference_foreground: None,
        },
        move |ui| {
            if let Some(size) = preview_size {
                ui.window().window().set_size(size);
            }
        },
    )?;
    Ok(())
}

fn safe_startup_error(error_value: &(dyn std::error::Error + 'static)) -> String {
    if error_value.downcast_ref::<NativeHostError>().is_some() {
        return "Argentum could not complete native startup. Check the workspace and provider settings, then try again.".into();
    }
    if let Some(error) = error_value.downcast_ref::<HostError>() {
        return match error {
            HostError::Platform(error) => safe_platform_startup_error(error),
            HostError::Security(_) => {
                "Argentum could not access the selected workspace. Choose an accessible directory with `argentum-cli workspace set PATH`.".into()
            }
            HostError::Store(_) => {
                "Argentum could not open its workspace database. Check the workspace and application-data permissions, then try again.".into()
            }
            HostError::Provider(_) | HostError::AndroidInference(_) => {
                "Argentum could not configure the selected provider. Check its endpoint and model in Settings, then try again.".into()
            }
            HostError::Runtime(_)
            | HostError::ChatGptAuth(_)
            | HostError::ProviderCredentialUnsupported
            | HostError::ProviderCredentialStorageUnavailable
            | HostError::ProviderCredentialTransactionUnavailable => {
                "Argentum could not complete startup. Check the workspace and provider settings, then try again.".into()
            }
        };
    }
    if let Some(error) = error_value.downcast_ref::<PlatformError>() {
        return safe_platform_startup_error(error);
    }
    "Argentum could not start. Check the workspace and provider settings, then try again.".into()
}

fn safe_platform_startup_error(error: &PlatformError) -> String {
    match error {
        PlatformError::InvalidWorkspaceConfiguration => {
            "The saved workspace selection is invalid. Choose a workspace with `argentum-cli workspace set PATH`, then restart Argentum.".into()
        }
        PlatformError::WorkspaceUnavailable => {
            "The selected workspace is unavailable. Choose an accessible directory with `argentum-cli workspace set PATH`, then restart Argentum.".into()
        }
        PlatformError::MissingDataDirectory => {
            "Argentum could not locate its application-data directory. Check the account environment and try again.".into()
        }
        PlatformError::DataDirectoryOverrideNotAbsolute => {
            "ARGENTUM_DATA_DIR must be an absolute path. Set it to a disposable absolute directory or unset it, then restart Argentum.".into()
        }
        PlatformError::SecureStorageUnavailable | PlatformError::SecureStorage(_) => {
            "Argentum could not access secure credential storage. Check the operating-system keyring or configure a provider through the host environment.".into()
        }
        PlatformError::InvalidProviderCredentialProfile => {
            "The saved provider credential profile is invalid. Review provider settings and try again.".into()
        }
        _ => "Argentum could not access its platform services. Check the workspace and provider settings, then try again.".into(),
    }
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

fn select_workspace(
    explicit: Option<PathBuf>,
    persisted: Option<&Path>,
    legacy: Option<&Path>,
    fallback: PathBuf,
) -> (PathBuf, WorkspaceSource) {
    if let Some(path) = explicit {
        return (path, WorkspaceSource::Explicit);
    }
    if let Some(path) = persisted {
        return (path.to_path_buf(), WorkspaceSource::Persisted);
    }
    if let Some(path) = legacy {
        return (path.to_path_buf(), WorkspaceSource::Legacy);
    }
    (fallback, WorkspaceSource::CurrentDirectory)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceSource {
    Explicit,
    Persisted,
    Legacy,
    CurrentDirectory,
}

fn canonical_workspace(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let canonical = std::fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err("the selected workspace is not a directory".into());
    }
    Ok(canonical)
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
        legacy_minimax_profile, parse_window_size, safe_startup_error, select_workspace,
        WorkspaceSource,
    };
    use argentum_app::ui_bridge::append_matching_stream_delta;
    use argentum_domain::AppEvent;
    use argentum_platform::PlatformError;
    use std::path::{Path, PathBuf};

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
        let (workspace, source) = select_workspace(
            Some(PathBuf::from(r"A:\explicit")),
            Some(Path::new(r"A:\persisted")),
            Some(Path::new(r"A:\legacy")),
            PathBuf::from(r"A:\fallback"),
        );

        assert_eq!(workspace, PathBuf::from(r"A:\explicit"));
        assert_eq!(source, WorkspaceSource::Explicit);
    }

    #[test]
    fn legacy_workspace_is_used_only_without_an_explicit_override() {
        let (workspace, source) = select_workspace(
            None,
            None,
            Some(Path::new(r"A:\legacy")),
            PathBuf::from(r"A:\fallback"),
        );

        assert_eq!(workspace, PathBuf::from(r"A:\legacy"));
        assert_eq!(source, WorkspaceSource::Legacy);
    }

    #[test]
    fn persisted_workspace_precedes_legacy_and_current_directory() {
        let (workspace, source) = select_workspace(
            None,
            Some(Path::new(r"A:\persisted")),
            Some(Path::new(r"A:\legacy")),
            PathBuf::from(r"A:\fallback"),
        );

        assert_eq!(workspace, PathBuf::from(r"A:\persisted"));
        assert_eq!(source, WorkspaceSource::Persisted);
    }

    #[test]
    fn startup_diagnostics_explain_how_to_recover_workspace_selection() {
        let message = safe_startup_error(&PlatformError::InvalidWorkspaceConfiguration);
        assert!(message.contains("workspace set PATH"));
        assert!(!message.contains("secret"));
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
