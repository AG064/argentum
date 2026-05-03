use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::Manager;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveSetupRequest {
    workspace_path: String,
    experience_level: String,
    runtime_mode: String,
    llm_provider: String,
    provider_api: String,
    provider_base_url: String,
    provider_model: String,
    provider_auth_method: String,
    provider_api_key: String,
    provider_api_key_env: String,
    custom_provider_name: String,
    selected_channels: Vec<String>,
    webchat_token: String,
    telegram_token: String,
    telegram_allowlist: String,
    whatsapp_phone_id: String,
    security_profile: String,
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveSetupResponse {
    status: String,
    config_path: String,
    secrets_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunDesktopActionRequest {
    action_id: String,
    workspace_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunDesktopActionResponse {
    status: String,
    message: String,
    command: String,
    output: String,
    pid: Option<String>,
    health_url: Option<String>,
    log_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopDefaultsResponse {
    default_workspace_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStateRequest {
    workspace_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStateResponse {
    workspace_path: String,
    config_path: String,
    workspace_ready: bool,
    config_exists: bool,
    data_exists: bool,
    logs_exists: bool,
    gateway_pid: Option<String>,
    gateway_log_preview: String,
    audit_log_preview: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestProviderRequest {
    provider: String,
    api: String,
    base_url: String,
    api_key: String,
    model: String,
    auth_method: Option<String>,
    workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestProviderResponse {
    status: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendChatMessageRequest {
    workspace_path: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SendChatMessageResponse {
    status: String,
    message: String,
    provider: String,
    model: String,
    offline: bool,
}

#[derive(Debug, Clone)]
struct ProviderRuntimeConfig {
    label: String,
    api: String,
    base_url: String,
    model: String,
    api_key_env: String,
    auth_method: String,
}

#[derive(Debug, Clone, Copy)]
struct ProviderDefaults {
    name: &'static str,
    label: &'static str,
    api: &'static str,
    base_url: &'static str,
    api_key_env: &'static str,
    default_model: &'static str,
    requires_key: bool,
}

fn ensure_safe_workspace(path: &str) -> Result<PathBuf, String> {
    let workspace = PathBuf::from(path);

    if path.trim().is_empty() {
        return Err("Workspace path is required".to_string());
    }

    if path.contains('\0') {
        return Err("Workspace path contains an invalid character".to_string());
    }

    if !workspace.is_absolute() {
        return Err("Workspace path must be absolute".to_string());
    }

    Ok(workspace)
}

fn ensure_existing_workspace(path: &str) -> Result<PathBuf, String> {
    let workspace = ensure_safe_workspace(path)?;
    if !workspace.exists() {
        return Err("Workspace path does not exist.".to_string());
    }

    Ok(workspace)
}

fn default_workspace_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data)
                .join("Programs")
                .join("Argentum")
                .join("workspace");
        }

        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(user_profile)
                .join("AppData")
                .join("Local")
                .join("Programs")
                .join("Argentum")
                .join("workspace");
        }
    }

    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Argentum")
                .join("workspace");
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("argentum")
            .join("workspace");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("argentum-workspace")
}

fn ensure_allowed(field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        return Ok(());
    }

    Err(format!("Invalid {field}: {value}"))
}

fn write_text(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

fn map_security_profile(profile: &str) -> &str {
    match profile {
        "ask" => "ask-every-time",
        "session" => "session-grant",
        other => other,
    }
}

fn render_config(request: &SaveSetupRequest) -> String {
    let profile = map_security_profile(&request.security_profile);
    let provider_name = selected_provider_name(request);
    let provider_label = selected_provider_defaults(request).label;
    let api_key_env = provider_api_key_env(request);
    let provider_api = provider_api(request);
    let provider_base_url = provider_base_url(request);
    let provider_model = provider_model(request);
    let provider_auth_method = provider_auth_method(request);
    let quoted_provider_label = yaml_quote(provider_label);
    let quoted_provider_base_url = yaml_quote(&provider_base_url);
    let quoted_provider_model = yaml_quote(&provider_model);
    let quoted_provider_auth_method = yaml_quote(&provider_auth_method);
    let quoted_workspace = yaml_quote(&request.workspace_path);
    let webchat_enabled = channel_enabled(request, "webchat");
    let telegram_enabled = channel_enabled(request, "telegram");
    let whatsapp_selected = channel_enabled(request, "whatsapp");
    let telegram_allowlist = request.telegram_allowlist.trim();
    let whatsapp_phone_id = request.whatsapp_phone_id.trim();
    let quoted_telegram_allowlist = yaml_quote(telegram_allowlist);
    let quoted_whatsapp_phone_id = yaml_quote(whatsapp_phone_id);
    format!(
        "version: \"{version}\"\nexperienceLevel: {experience}\nruntimeMode: {runtime}\nlogging:\n  level: info\n  format: json\nllm:\n  default: {provider_name}\n  providers:\n    {provider_name}:\n      label: {provider_label}\n      base_url: {provider_base_url}\n      api_key_env: {api_key_env}\n      api: {provider_api}\n      auth_method: {provider_auth_method}\n      models:\n        - {provider_model}\nsecurity:\n  capabilities:\n    defaultProfile: {profile}\n    workspaceRoot: {workspace}\n    auditPath: ./data/audit/capabilities.log\nfeatures:\n  webchat:\n    enabled: {webchat}\n  whatsapp-bridge:\n    enabled: false\n    selected: {whatsapp_selected}\n    phoneNumberId: {whatsapp_phone_id}\nchannels:\n  local:\n    enabled: true\n  webchat:\n    enabled: {webchat}\n  telegram:\n    enabled: {telegram}\n    allowlist: {telegram_allowlist}\n  whatsapp:\n    enabled: false\n    selected: {whatsapp_selected}\n",
        version = request.version,
        experience = request.experience_level,
        runtime = request.runtime_mode,
        provider_name = provider_name,
        provider_label = quoted_provider_label,
        provider_base_url = quoted_provider_base_url,
        api_key_env = api_key_env,
        provider_api = provider_api,
        provider_auth_method = quoted_provider_auth_method,
        provider_model = quoted_provider_model,
        profile = profile,
        workspace = quoted_workspace,
        webchat = webchat_enabled,
        telegram = telegram_enabled,
        telegram_allowlist = quoted_telegram_allowlist,
        whatsapp_selected = whatsapp_selected,
        whatsapp_phone_id = quoted_whatsapp_phone_id,
    )
}

fn provider_defaults(provider: &str) -> Option<ProviderDefaults> {
    match provider {
        "openai" => Some(ProviderDefaults {
            name: "openai",
            label: "OpenAI",
            api: "openai",
            base_url: "https://api.openai.com/v1",
            api_key_env: "OPENAI_API_KEY",
            default_model: "gpt-5.5",
            requires_key: true,
        }),
        "anthropic" => Some(ProviderDefaults {
            name: "anthropic",
            label: "Anthropic Claude",
            api: "anthropic",
            base_url: "https://api.anthropic.com",
            api_key_env: "ANTHROPIC_API_KEY",
            default_model: "claude-sonnet-4-20250514",
            requires_key: true,
        }),
        "google" => Some(ProviderDefaults {
            name: "google",
            label: "Google Gemini",
            api: "openai",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/",
            api_key_env: "GOOGLE_API_KEY",
            default_model: "gemini-2.5-flash",
            requires_key: true,
        }),
        "openrouter" => Some(ProviderDefaults {
            name: "openrouter",
            label: "OpenRouter",
            api: "openai",
            base_url: "https://openrouter.ai/api/v1",
            api_key_env: "OPENROUTER_API_KEY",
            default_model: "google/gemma-3-27b-it",
            requires_key: true,
        }),
        "nvidia" => Some(ProviderDefaults {
            name: "nvidia",
            label: "NVIDIA",
            api: "openai",
            base_url: "https://integrate.api.nvidia.com/v1",
            api_key_env: "NVIDIA_API_KEY",
            default_model: "deepseek-ai/deepseek-v3.2",
            requires_key: true,
        }),
        "groq" => Some(ProviderDefaults {
            name: "groq",
            label: "Groq",
            api: "openai",
            base_url: "https://api.groq.com/openai/v1",
            api_key_env: "GROQ_API_KEY",
            default_model: "meta-llama/llama-4-scout-17b-16e-instruct",
            requires_key: true,
        }),
        "minimax" => Some(ProviderDefaults {
            name: "minimax",
            label: "MiniMax",
            api: "openai",
            base_url: "https://api.minimax.io/v1",
            api_key_env: "MINIMAX_API_KEY",
            default_model: "MiniMax-M2.7",
            requires_key: true,
        }),
        "ollama" => Some(ProviderDefaults {
            name: "ollama",
            label: "Ollama / local",
            api: "openai",
            base_url: "http://127.0.0.1:11434/v1",
            api_key_env: "OLLAMA_API_KEY",
            default_model: "llama3.1",
            requires_key: false,
        }),
        "custom" => Some(ProviderDefaults {
            name: "custom",
            label: "Custom endpoint",
            api: "openai",
            base_url: "http://127.0.0.1:8000/v1",
            api_key_env: "CUSTOM_API_KEY",
            default_model: "custom-model",
            requires_key: false,
        }),
        _ => None,
    }
}

fn selected_provider_name(request: &SaveSetupRequest) -> String {
    if request.llm_provider == "custom" {
        let cleaned = request.custom_provider_name.trim();
        if cleaned.is_empty() {
            "custom".to_string()
        } else {
            cleaned
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                        character.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect()
        }
    } else {
        selected_provider_defaults(request).name.to_string()
    }
}

fn selected_provider_defaults(request: &SaveSetupRequest) -> ProviderDefaults {
    provider_defaults(&request.llm_provider).unwrap_or_else(|| provider_defaults("openai").unwrap())
}

fn provider_api_key_env(request: &SaveSetupRequest) -> String {
    if request.llm_provider == "custom" {
        let value = request.provider_api_key_env.trim();
        if value.is_empty() {
            "CUSTOM_API_KEY".to_string()
        } else {
            value.to_ascii_uppercase()
        }
    } else {
        selected_provider_defaults(request).api_key_env.to_string()
    }
}

fn validate_env_name(value: &str) -> Result<(), String> {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err("Provider secret variable name is required".to_string());
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(
            "Provider secret variable name must start with a letter or underscore".to_string(),
        );
    }

    if characters.any(|character| !(character.is_ascii_alphanumeric() || character == '_')) {
        return Err(
            "Provider secret variable name can contain only letters, numbers, and underscores"
                .to_string(),
        );
    }

    Ok(())
}

fn provider_base_url(request: &SaveSetupRequest) -> String {
    let defaults = selected_provider_defaults(request);
    let value = request.provider_base_url.trim();
    if value.is_empty() {
        defaults.base_url.to_string()
    } else {
        value.to_string()
    }
}

fn provider_model(request: &SaveSetupRequest) -> String {
    let defaults = selected_provider_defaults(request);
    let value = request.provider_model.trim();
    if value.is_empty() {
        defaults.default_model.to_string()
    } else {
        value.to_string()
    }
}

fn provider_api(request: &SaveSetupRequest) -> String {
    let defaults = selected_provider_defaults(request);
    let value = request.provider_api.trim();
    if value.is_empty() {
        defaults.api.to_string()
    } else {
        value.to_string()
    }
}

fn provider_auth_method(request: &SaveSetupRequest) -> String {
    let value = request.provider_auth_method.trim();
    if value.is_empty() {
        "api-key".to_string()
    } else {
        value.to_string()
    }
}

fn ensure_provider_auth_method(method: &str) -> Result<(), String> {
    match method {
        "api-key" => Ok(()),
        "browser-account" => Err(
            "Browser account authorization is not supported for direct model calls yet. Use API key authentication until the Codex OAuth provider is implemented."
                .to_string(),
        ),
        other => Err(format!("Invalid provider authorization method: {other}")),
    }
}

fn channel_enabled(request: &SaveSetupRequest, channel: &str) -> bool {
    request
        .selected_channels
        .iter()
        .any(|selected| selected == channel)
}

fn format_secret(value: &str) -> String {
    if value.chars().any(|character| {
        character.is_whitespace() || character == '"' || character == '\'' || character == '#'
    }) {
        format!("{:?}", value)
    } else {
        value.to_string()
    }
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn target_triple() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

fn sidecar_file_name() -> String {
    let extension = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    format!("argentum-cli-{}{}", target_triple(), extension)
}

fn sidecar_file_names() -> Vec<String> {
    let installed_name = if cfg!(target_os = "windows") {
        "argentum-cli.exe".to_string()
    } else {
        "argentum-cli".to_string()
    };
    vec![installed_name, sidecar_file_name()]
}

fn resolve_sidecar_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    for file_name in sidecar_file_names() {
        if let Ok(resource_dir) = app.path().resource_dir() {
            candidates.push(resource_dir.join(&file_name));
            candidates.push(resource_dir.join("binaries").join(&file_name));
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                candidates.push(exe_dir.join(&file_name));
                candidates.push(exe_dir.join("binaries").join(&file_name));
            }
        }

        if let Ok(current_dir) = std::env::current_dir() {
            candidates.push(current_dir.join("binaries").join(&file_name));
            candidates.push(current_dir.join("src").join("desktop").join("binaries").join(&file_name));
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| {
            "Argentum CLI sidecar is missing. Reinstall Argentum or rebuild the desktop bundle."
                .to_string()
        })
}

fn plain_command(args: &[&str]) -> String {
    format!("argentum {}", args.join(" "))
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            while let Some(next) = chars.next() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        output.push(character);
    }

    output.replace("âœ“", "OK")
        .replace("â„¹", "Info")
        .replace("âš ", "Warning")
}

fn run_sidecar(
    app: &tauri::AppHandle,
    workspace: &Path,
    args: &[&str],
) -> Result<String, String> {
    let sidecar = resolve_sidecar_path(app)?;
    let output = Command::new(sidecar)
        .args(args)
        .env("ARGENTUM_WORKDIR", workspace)
        .env("ARGENTUM_SKIP_EXIT_PAUSE", "1")
        .env("AGCLAW_WORKDIR", "")
        .env("AGCLAW_SKIP_EXIT_PAUSE", "1")
        .current_dir(workspace)
        .output()
        .map_err(|error| format!("Failed to run Argentum sidecar: {error}"))?;

    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    let combined = [stdout.trim(), stderr.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if output.status.success() {
        Ok(redact_sensitive_output(&combined))
    } else if combined.is_empty() {
        Err(format!(
            "{} failed with exit code {}.",
            plain_command(args),
            output.status.code().unwrap_or(-1)
        ))
    } else {
        Err(redact_sensitive_output(&combined))
    }
}

fn gateway_port(workspace: &Path) -> u16 {
    let config_path = workspace.join("config").join("default.yaml");
    let Ok(contents) = std::fs::read_to_string(config_path) else {
        return 3000;
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("port:") {
            if let Ok(port) = value.trim().parse::<u16>() {
                return port;
            }
        }
    }

    3000
}

fn parse_gateway_pid(output: &str) -> Option<String> {
    let marker = "PID:";
    let start = output.find(marker)? + marker.len();
    let pid = output[start..]
        .chars()
        .skip_while(|character| character.is_whitespace())
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();

    if pid.is_empty() {
        None
    } else {
        Some(pid)
    }
}

fn check_gateway_port(port: u16) -> Result<(), String> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(_) => Err(format!(
            "Gateway failed to start because port {port} is already in use."
        )),
    }
}

fn redact_sensitive_output(output: &str) -> String {
    output
        .lines()
        .map(redact_sensitive_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_sensitive_line(line: &str) -> String {
    let normalized = line.to_ascii_lowercase();
    let sensitive_markers = [
        "api_key",
        "apikey",
        "authorization",
        "bearer ",
        "password",
        "secret",
        "token",
    ];

    if sensitive_markers
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        return "[redacted sensitive log line]".to_string();
    }

    line.to_string()
}

fn read_preview(path: &Path, max_lines: usize) -> String {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return "No entries yet.".to_string();
    };

    let lines = contents.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);

    lines[start..]
        .iter()
        .map(|line| redact_sensitive_line(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_gateway_pid(path: &Path) -> Option<String> {
    let Ok(pid) = std::fs::read_to_string(path) else {
        return None;
    };
    let trimmed = pid.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_local_endpoint(base_url: &str) -> bool {
    let normalized = base_url.to_ascii_lowercase();
    normalized.contains("127.0.0.1")
        || normalized.contains("localhost")
        || normalized.contains("[::1]")
}

fn models_url(base_url: &str, api: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if api == "anthropic" {
        if trimmed.ends_with("/v1") {
            format!("{trimmed}/models")
        } else {
            format!("{trimmed}/v1/models")
        }
    } else {
        format!("{trimmed}/models")
    }
}

fn redact_provider_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        return "Provider test timed out before a response.".to_string();
    }

    if error.is_connect() {
        return "Provider endpoint could not be reached.".to_string();
    }

    "Provider request failed before a usable response was returned.".to_string()
}

fn read_secret(workspace: &Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(workspace.join("secrets.env")).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || !trimmed.contains('=') {
            continue;
        }
        let (name, value) = trimmed.split_once('=')?;
        if name.trim() == key {
            let clean = value.trim().trim_matches('"').trim_matches('\'').to_string();
            if clean.is_empty() {
                return None;
            }
            return Some(clean);
        }
    }

    None
}

fn yaml_string_at<'a>(value: &'a serde_yaml::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }

    current.as_str()
}

fn provider_runtime_config(workspace: &Path) -> Result<ProviderRuntimeConfig, String> {
    let config_path = workspace.join("config").join("default.yaml");
    let contents = std::fs::read_to_string(&config_path)
        .map_err(|_| "Configuration file is missing. Finish onboarding first.".to_string())?;
    let yaml = serde_yaml::from_str::<serde_yaml::Value>(&contents)
        .map_err(|_| "Configuration file could not be read. Review config/default.yaml.".to_string())?;

    let provider_name = yaml_string_at(&yaml, &["llm", "default"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "No default provider is configured.".to_string())?
        .to_string();
    let provider = yaml
        .get("llm")
        .and_then(|llm| llm.get("providers"))
        .and_then(|providers| providers.get(&provider_name))
        .ok_or_else(|| format!("Provider '{provider_name}' is missing from config/default.yaml."))?;

    let model = provider
        .get("models")
        .and_then(|models| models.as_sequence())
        .and_then(|models| models.first())
        .and_then(|model| model.as_str())
        .ok_or_else(|| format!("Provider '{provider_name}' has no model configured."))?;

    Ok(ProviderRuntimeConfig {
        label: provider
            .get("label")
            .and_then(|value| value.as_str())
            .unwrap_or("Configured provider")
            .to_string(),
        api: provider
            .get("api")
            .and_then(|value| value.as_str())
            .unwrap_or("openai")
            .to_string(),
        base_url: provider
            .get("base_url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Provider endpoint is missing from config/default.yaml.".to_string())?
            .to_string(),
        model: model.to_string(),
        api_key_env: provider
            .get("api_key_env")
            .and_then(|value| value.as_str())
            .unwrap_or("OPENAI_API_KEY")
            .to_string(),
        auth_method: provider
            .get("auth_method")
            .and_then(|value| value.as_str())
            .unwrap_or("api-key")
            .to_string(),
    })
}

fn provider_api_key(
    workspace: Option<&Path>,
    request_key: &str,
    key_env: &str,
) -> Option<String> {
    let trimmed = request_key.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }

    workspace
        .and_then(|path| read_secret(path, key_env))
        .or_else(|| std::env::var(key_env).ok())
        .filter(|value| !value.trim().is_empty())
}

fn chat_url(base_url: &str, api: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if api == "anthropic" {
        if trimmed.ends_with("/v1") {
            format!("{trimmed}/messages")
        } else {
            format!("{trimmed}/v1/messages")
        }
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn offline_chat_message(request: &str, reason: &str) -> String {
    let lower = request.to_ascii_lowercase();

    if lower.contains("gateway") {
        return format!(
            "Offline mode: {reason} Use the Gateway page or Start Gateway button to start, stop, or inspect the local gateway."
        );
    }

    if lower.contains("security") || lower.contains("permission") || lower.contains("access") {
        return format!(
            "Offline mode: {reason} Security stays restricted to the selected workspace unless you approve a capability."
        );
    }

    if lower.contains("provider") || lower.contains("api") || lower.contains("model") {
        return format!(
            "Offline mode: {reason} Open Settings, add or test the provider, then send the message again."
        );
    }

    format!(
        "Offline mode: {reason} I can still help with setup, security, provider testing, gateway actions, diagnostics, and logs."
    )
}

fn parse_openai_chat_response(value: serde_json::Value) -> Result<String, String> {
    value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "Provider returned an empty chat response.".to_string())
}

fn parse_anthropic_chat_response(value: serde_json::Value) -> Result<String, String> {
    value
        .get("content")
        .and_then(|content| content.as_array())
        .and_then(|content| content.first())
        .and_then(|block| block.get("text"))
        .and_then(|text| text.as_str())
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "Provider returned an empty chat response.".to_string())
}

fn provider_error_detail(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(|message| message.as_str())
        {
            return Some(redact_sensitive_output(message.trim()));
        }

        if let Some(message) = value.get("message").and_then(|message| message.as_str()) {
            return Some(redact_sensitive_output(message.trim()));
        }
    }

    Some(redact_sensitive_output(trimmed.lines().next().unwrap_or(trimmed)))
}

fn provider_http_error(provider: &str, status: u16, body: &str) -> String {
    let detail = provider_error_detail(body);
    let suffix = detail
        .filter(|message| !message.is_empty())
        .map(|message| format!(" Provider said: {message}"))
        .unwrap_or_default();

    match status {
        401 | 403 => format!("{provider} rejected the API key.{suffix}"),
        429 => format!(
            "{provider} hit a rate or quota limit (HTTP 429). Wait a minute and retry, choose a smaller model, or check billing/usage limits for the selected key.{suffix}"
        ),
        404 => format!(
            "{provider} returned HTTP 404. Check that the endpoint URL and selected model are available.{suffix}"
        ),
        _ => format!(
            "{provider} responded with HTTP {status}. Check the endpoint and selected model, then test again.{suffix}"
        ),
    }
}

#[tauri::command]
async fn test_provider(request: TestProviderRequest) -> Result<TestProviderResponse, String> {
    ensure_allowed("provider API", &request.api, &["openai", "anthropic"])?;
    ensure_provider_auth_method(request.auth_method.as_deref().unwrap_or("api-key"))?;

    let defaults = provider_defaults(&request.provider)
        .unwrap_or_else(|| provider_defaults("custom").expect("custom provider defaults"));
    let workspace = match request.workspace_path.as_deref() {
        Some(path) if !path.trim().is_empty() => Some(ensure_existing_workspace(path)?),
        _ => None,
    };
    let base_url = if request.base_url.trim().is_empty() {
        defaults.base_url
    } else {
        request.base_url.trim()
    };
    let model = if request.model.trim().is_empty() {
        defaults.default_model
    } else {
        request.model.trim()
    };

    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err("Provider endpoint must start with http:// or https://".to_string());
    }

    let api_key = provider_api_key(workspace.as_deref(), &request.api_key, defaults.api_key_env)
        .unwrap_or_default();

    if defaults.requires_key && api_key.trim().is_empty() {
        return Err(format!(
            "{} needs an API key before it can be tested.",
            defaults.label
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|_| "Provider test client could not be created.".to_string())?;

    let url = models_url(base_url, &request.api);
    let mut builder = client.get(url);

    if !api_key.is_empty() {
        builder = if request.api == "anthropic" {
            builder
                .header("x-api-key", api_key.as_str())
                .header("anthropic-version", "2023-06-01")
        } else {
            builder.bearer_auth(api_key.as_str())
        };
    }

    let response = builder.send().await.map_err(redact_provider_error)?;
    let status = response.status();

    if status.is_success() {
        return Ok(TestProviderResponse {
            status: "ok".to_string(),
            message: format!(
                "{} responded and model '{}' is ready to configure.",
                defaults.label, model
            ),
        });
    }

    let error_body = response.text().await.unwrap_or_default();

    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(provider_http_error(defaults.label, status.as_u16(), &error_body));
    }

    if status.as_u16() == 404 && is_local_endpoint(base_url) {
        return Ok(TestProviderResponse {
            status: "warning".to_string(),
            message: "Local endpoint is reachable, but /models was not found. You can continue in offline guided mode or check your local server.".to_string(),
        });
    }

    Err(provider_http_error(defaults.label, status.as_u16(), &error_body))
}

#[tauri::command]
async fn send_chat_message(
    request: SendChatMessageRequest,
) -> Result<SendChatMessageResponse, String> {
    let workspace = ensure_existing_workspace(&request.workspace_path)?;
    let message = request.message.trim();

    if message.is_empty() {
        return Err("Message is required.".to_string());
    }

    let config = match provider_runtime_config(&workspace) {
        Ok(config) => config,
        Err(error) => {
            return Ok(SendChatMessageResponse {
                status: "offline".to_string(),
                message: offline_chat_message(message, &error),
                provider: "Offline".to_string(),
                model: "local-guided".to_string(),
                offline: true,
            });
        }
    };
    ensure_provider_auth_method(&config.auth_method)?;

    let api_key =
        provider_api_key(Some(&workspace), "", &config.api_key_env).unwrap_or_default();
    let requires_key = !is_local_endpoint(&config.base_url);

    if requires_key && api_key.trim().is_empty() {
        return Ok(SendChatMessageResponse {
            status: "offline".to_string(),
            message: offline_chat_message(
                message,
                &format!("{} is missing an API key.", config.label),
            ),
            provider: config.label,
            model: config.model,
            offline: true,
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|_| "Chat client could not be created.".to_string())?;

    let url = chat_url(&config.base_url, &config.api);
    let mut builder = client.post(url);

    if !api_key.is_empty() {
        builder = if config.api == "anthropic" {
            builder
                .header("x-api-key", api_key.as_str())
                .header("anthropic-version", "2023-06-01")
        } else {
            builder.bearer_auth(api_key.as_str())
        };
    }

    let body = if config.api == "anthropic" {
        json!({
            "model": config.model,
            "max_tokens": 900,
            "messages": [
                { "role": "user", "content": message }
            ]
        })
    } else {
        json!({
            "model": config.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are Argentum, a secure desktop AI agent. Be direct, practical, and stay within the user's configured workspace and permissions."
                },
                { "role": "user", "content": message }
            ],
            "temperature": 0.4
        })
    };

    let response = builder
        .json(&body)
        .send()
        .await
        .map_err(redact_provider_error)?;
    let status = response.status();

    if status.as_u16() == 401 || status.as_u16() == 403 {
        let error_body = response.text().await.unwrap_or_default();
        return Err(provider_http_error(&config.label, status.as_u16(), &error_body));
    }

    if !status.is_success() {
        let error_body = response.text().await.unwrap_or_default();
        return Err(provider_http_error(&config.label, status.as_u16(), &error_body));
    }

    let value = response
        .json::<serde_json::Value>()
        .await
        .map_err(|_| "Provider returned a response Argentum could not read.".to_string())?;
    let answer = if config.api == "anthropic" {
        parse_anthropic_chat_response(value)?
    } else {
        parse_openai_chat_response(value)?
    };

    Ok(SendChatMessageResponse {
        status: "ok".to_string(),
        message: answer,
        provider: config.label,
        model: config.model,
        offline: false,
    })
}

#[tauri::command]
fn desktop_defaults() -> DesktopDefaultsResponse {
    DesktopDefaultsResponse {
        default_workspace_path: default_workspace_path().display().to_string(),
    }
}

#[tauri::command]
fn desktop_state(request: DesktopStateRequest) -> Result<DesktopStateResponse, String> {
    let workspace = ensure_safe_workspace(&request.workspace_path)?;
    let config_path = workspace.join("config/default.yaml");
    let data_dir = workspace.join("data");
    let logs_dir = workspace.join("logs");
    let gateway_pid_path = data_dir.join(".gateway.pid");
    let data_gateway_log_path = data_dir.join("gateway.log");
    let logs_gateway_log_path = logs_dir.join("gateway.log");
    let gateway_log_path = if data_gateway_log_path.exists() {
        data_gateway_log_path
    } else {
        logs_gateway_log_path
    };
    let audit_log_path = data_dir.join("audit").join("capabilities.log");

    Ok(DesktopStateResponse {
        workspace_path: workspace.display().to_string(),
        config_path: config_path.display().to_string(),
        workspace_ready: workspace.exists(),
        config_exists: config_path.exists(),
        data_exists: data_dir.exists(),
        logs_exists: logs_dir.exists(),
        gateway_pid: read_gateway_pid(&gateway_pid_path),
        gateway_log_preview: read_preview(&gateway_log_path, 12),
        audit_log_preview: read_preview(&audit_log_path, 12),
    })
}

#[tauri::command]
fn save_setup(request: SaveSetupRequest) -> Result<SaveSetupResponse, String> {
    let workspace = ensure_safe_workspace(&request.workspace_path)?;
    ensure_allowed(
        "experience level",
        &request.experience_level,
        &["beginner", "comfortable", "expert"],
    )?;
    ensure_allowed(
        "runtime mode",
        &request.runtime_mode,
        &["desktop", "cli", "service"],
    )?;
    ensure_allowed(
        "LLM provider",
        &request.llm_provider,
        &[
            "openai",
            "anthropic",
            "google",
            "openrouter",
            "nvidia",
            "groq",
            "minimax",
            "ollama",
            "custom",
        ],
    )?;
    ensure_allowed(
        "provider API",
        &request.provider_api,
        &["openai", "anthropic"],
    )?;
    ensure_provider_auth_method(&provider_auth_method(&request))?;
    validate_env_name(&provider_api_key_env(&request))?;
    if request.provider_base_url.trim().is_empty() && request.llm_provider == "custom" {
        return Err("Custom provider endpoint is required".to_string());
    }
    for channel in &request.selected_channels {
        ensure_allowed(
            "channel",
            channel,
            &["local", "webchat", "telegram", "whatsapp"],
        )?;
    }
    ensure_allowed(
        "security profile",
        &request.security_profile,
        &["restricted", "ask", "session", "trusted"],
    )?;

    let config_dir = workspace.join("config");
    let data_dir = workspace.join("data");
    let logs_dir = workspace.join("logs");
    let audit_dir = data_dir.join("audit");

    std::fs::create_dir_all(&config_dir)
        .map_err(|error| format!("Failed to create config directory: {error}"))?;
    std::fs::create_dir_all(&audit_dir)
        .map_err(|error| format!("Failed to create audit directory: {error}"))?;
    std::fs::create_dir_all(&logs_dir)
        .map_err(|error| format!("Failed to create logs directory: {error}"))?;

    let config_path = workspace.join("config/default.yaml");
    let secrets_path = workspace.join("secrets.env");
    let mut secrets = vec![
        "# Argentum secrets are stored outside YAML.".to_string(),
        "# Provider keys are added by the desktop credential flow.".to_string(),
    ];

    if !request.provider_api_key.trim().is_empty() {
        secrets.push(format!(
            "{}={}",
            provider_api_key_env(&request),
            format_secret(request.provider_api_key.trim())
        ));
    }

    if !request.webchat_token.trim().is_empty() {
        secrets.push(format!(
            "ARGENTUM_WEBCHAT_AUTH_TOKEN={}",
            format_secret(request.webchat_token.trim())
        ));
    }

    if !request.telegram_token.trim().is_empty() {
        secrets.push(format!(
            "ARGENTUM_TELEGRAM_TOKEN={}",
            format_secret(request.telegram_token.trim())
        ));
    }

    write_text(&config_path, &render_config(&request))?;
    write_text(&secrets_path, &format!("{}\n", secrets.join("\n")))?;

    Ok(SaveSetupResponse {
        status: "setup_saved".to_string(),
        config_path: config_path.display().to_string(),
        secrets_path: secrets_path.display().to_string(),
    })
}

fn gateway_response(
    status: &str,
    message: String,
    args: &[&str],
    output: String,
    pid: Option<String>,
    health_url: Option<String>,
    log_path: &Path,
) -> RunDesktopActionResponse {
    RunDesktopActionResponse {
        status: status.to_string(),
        message,
        command: plain_command(args),
        output,
        pid,
        health_url,
        log_path: Some(log_path.display().to_string()),
    }
}

fn run_gateway_action(
    app: &tauri::AppHandle,
    workspace: &Path,
    action_id: &str,
) -> Result<RunDesktopActionResponse, String> {
    std::fs::create_dir_all(workspace.join("data"))
        .map_err(|error| format!("Failed to create gateway data directory: {error}"))?;

    let port = gateway_port(workspace);
    let health_url = format!("http://127.0.0.1:{port}/health");
    let log_path = workspace.join("data").join("gateway.log");

    match action_id {
        "gateway-status" => {
            let args = ["gateway", "status"];
            let output = run_sidecar(app, workspace, &args)?;
            let pid = parse_gateway_pid(&output);
            let message = match &pid {
                Some(pid) => format!("Gateway running on {health_url} (PID: {pid})."),
                None => "Gateway is stopped.".to_string(),
            };
            let status = if pid.is_some() { "running" } else { "stopped" };

            Ok(gateway_response(
                status,
                message,
                &args,
                output,
                pid,
                Some(health_url),
                &log_path,
            ))
        }
        "gateway-start" => {
            let status_args = ["gateway", "status"];
            let status_output = run_sidecar(app, workspace, &status_args)?;
            if let Some(pid) = parse_gateway_pid(&status_output) {
                return Ok(gateway_response(
                    "running",
                    format!("Gateway is already running on {health_url} (PID: {pid})."),
                    &status_args,
                    status_output,
                    Some(pid),
                    Some(health_url),
                    &log_path,
                ));
            }

            check_gateway_port(port)?;

            let port_text = port.to_string();
            let start_args = ["gateway", "start", "--port", port_text.as_str()];
            let start_output = run_sidecar(app, workspace, &start_args)?;
            std::thread::sleep(Duration::from_millis(700));
            let after_output = run_sidecar(app, workspace, &status_args).unwrap_or_default();
            let pid = parse_gateway_pid(&after_output).or_else(|| parse_gateway_pid(&start_output));

            let Some(pid) = pid else {
                return Err(
                    "Gateway failed to start. Check the gateway log for details.".to_string(),
                );
            };

            let output = [start_output.trim(), after_output.trim()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");

            Ok(gateway_response(
                "running",
                format!("Gateway started on {health_url} (PID: {pid})."),
                &start_args,
                output,
                Some(pid),
                Some(health_url),
                &log_path,
            ))
        }
        "gateway-stop" => {
            let args = ["gateway", "stop"];
            let output = run_sidecar(app, workspace, &args)?;
            Ok(gateway_response(
                "stopped",
                "Gateway stopped.".to_string(),
                &args,
                output,
                None,
                Some(health_url),
                &log_path,
            ))
        }
        "gateway-logs" => {
            let args = ["gateway", "logs", "-n", "100"];
            let output = run_sidecar(app, workspace, &args)?;
            Ok(gateway_response(
                "ok",
                format!("Showing recent gateway logs from {}.", log_path.display()),
                &args,
                output,
                None,
                Some(health_url),
                &log_path,
            ))
        }
        _ => Err(format!("Unknown desktop action: {action_id}")),
    }
}

#[tauri::command]
fn run_desktop_action(
    app: tauri::AppHandle,
    request: RunDesktopActionRequest,
) -> Result<RunDesktopActionResponse, String> {
    let workspace = ensure_existing_workspace(&request.workspace_path)?;
    run_gateway_action(&app, &workspace, &request.action_id)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            save_setup,
            test_provider,
            send_chat_message,
            run_desktop_action,
            desktop_defaults,
            desktop_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argentum");
}
