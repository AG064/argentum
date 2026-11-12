//! The messages shown when Argentum cannot start. They name what to check
//! and never include paths, keys, or raw error text.

use argentum_cli::HostError;
use argentum_platform::PlatformError;

use crate::NativeHostError;

/// A plain message for the owner about why startup failed.
pub fn startup_error_message(error_value: &(dyn std::error::Error + 'static)) -> String {
    if matches!(
        error_value.downcast_ref::<NativeHostError>(),
        Some(NativeHostError::RecoveryRequired)
    ) {
        return "Argentum did not close cleanly last time, or another copy is still running, so it will not take over this data on its own. Close any other copy, then run 'argentum recovery inspect' for this database and follow the command it gives.".into();
    }
    if matches!(
        error_value.downcast_ref::<NativeHostError>(),
        Some(NativeHostError::SavedData)
    ) {
        return "Argentum could not read its saved data, so it stopped without changing anything. Install the latest version and open it again. If this keeps happening, the app log has the details.".into();
    }
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

/// The page for a new GitHub issue, with the version and platform filled in.
/// Only those two facts go in; nothing from sessions or settings is added.
pub fn issue_report_url(platform: &str) -> String {
    let body = format!(
        "Version: {}\nPlatform: {platform}\n\nWhat happened:\n\nWhat you expected:\n",
        env!("CARGO_PKG_VERSION")
    );
    let encoded: String = body
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    format!("https://github.com/AG064/argentum/issues/new?body={encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_saved_data_gets_its_own_message() {
        let message = startup_error_message(&NativeHostError::SavedData);
        assert!(message.contains("could not read its saved data"));
        assert!(message.contains("without changing anything"));
    }

    #[test]
    fn the_issue_page_carries_only_version_and_platform() {
        let url = issue_report_url("windows");
        assert!(url.starts_with("https://github.com/AG064/argentum/issues/new?body="));
        assert!(url.contains("Platform%3A%20windows"));
        assert!(!url.contains(' ') && !url.contains('\n'));
    }
}
