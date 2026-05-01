use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestProviderResponse {
    status: String,
    message: String,
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
    let quoted_provider_label = yaml_quote(provider_label);
    let quoted_provider_base_url = yaml_quote(&provider_base_url);
    let quoted_provider_model = yaml_quote(&provider_model);
    let quoted_workspace = yaml_quote(&request.workspace_path);
    let webchat_enabled = channel_enabled(request, "webchat");
    let telegram_enabled = channel_enabled(request, "telegram");
    let whatsapp_selected = channel_enabled(request, "whatsapp");
    let telegram_allowlist = request.telegram_allowlist.trim();
    let whatsapp_phone_id = request.whatsapp_phone_id.trim();
    let quoted_telegram_allowlist = yaml_quote(telegram_allowlist);
    let quoted_whatsapp_phone_id = yaml_quote(whatsapp_phone_id);
    format!(
        "version: \"{version}\"\nexperienceLevel: {experience}\nruntimeMode: {runtime}\nllm:\n  default: {provider_name}\n  providers:\n    {provider_name}:\n      label: {provider_label}\n      base_url: {provider_base_url}\n      api_key_env: {api_key_env}\n      api: {provider_api}\n      models:\n        - {provider_model}\nsecurity:\n  capabilities:\n    defaultProfile: {profile}\n    workspaceRoot: {workspace}\n    auditPath: ./data/audit/capabilities.log\nfeatures:\n  webchat:\n    enabled: {webchat}\n  whatsapp-bridge:\n    enabled: false\n    selected: {whatsapp_selected}\n    phoneNumberId: {whatsapp_phone_id}\nchannels:\n  local:\n    enabled: true\n  webchat:\n    enabled: {webchat}\n  telegram:\n    enabled: {telegram}\n    allowlist: {telegram_allowlist}\n  whatsapp:\n    enabled: false\n    selected: {whatsapp_selected}\n",
        version = request.version,
        experience = request.experience_level,
        runtime = request.runtime_mode,
        provider_name = provider_name,
        provider_label = quoted_provider_label,
        provider_base_url = quoted_provider_base_url,
        api_key_env = api_key_env,
        provider_api = provider_api,
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
            default_model: "gpt-4o-mini",
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

fn desktop_action(action_id: &str) -> Option<(&'static str, &'static str)> {
    match action_id {
        "chat-start" => Some((
            "argentum gateway start",
            "Gateway start is prepared. Argentum will request network and service permissions before launching it.",
        )),
        "gateway-status" => Some((
            "argentum gateway status",
            "Gateway status check is ready for the selected workspace.",
        )),
        "agents-list" => Some((
            "argentum agents list",
            "Agent profile list is ready for the selected workspace.",
        )),
        "agents-create" => Some((
            "argentum agents create --name \"researcher\"",
            "Agent profile creation requires a profile name and config write approval.",
        )),
        "runner-acp" => Some((
            "argentum acp run \"code\"",
            "Sandbox run is prepared and requires explicit code plus approval before execution.",
        )),
        "cron-list" => Some((
            "argentum cron list",
            "Scheduled job list is ready for the selected workspace.",
        )),
        "skills-list" => Some((
            "argentum skill list",
            "Installed skill list is ready for the selected workspace.",
        )),
        "skills-search" => Some((
            "argentum skill search \"browser\"",
            "Skill search may use network access and will stay gated by approval.",
        )),
        "webchat-config" => Some((
            "argentum config features.webchat",
            "Webchat configuration review is ready.",
        )),
        "telegram-status" => Some((
            "argentum telegram status",
            "Telegram status check is ready without exposing bot secrets.",
        )),
        "graph-feature" => Some((
            "argentum feature knowledge-graph",
            "Knowledge graph feature inspection is ready.",
        )),
        "memory-search" => Some((
            "argentum memory search \"project context\"",
            "Memory search is prepared for workspace-scoped recall.",
        )),
        "memory-list" => Some((
            "argentum memory list",
            "Memory namespace list is ready.",
        )),
        "logs-gateway" => Some((
            "argentum gateway logs --lines 100",
            "Gateway log review is ready and remains workspace-scoped.",
        )),
        "security-status" => Some((
            "argentum security status",
            "Security overview is ready.",
        )),
        "security-approvals" => Some((
            "argentum security approvals",
            "Approval queue review is ready.",
        )),
        "security-audit" => Some((
            "argentum security audit",
            "Security audit log review is ready.",
        )),
        "settings-config" => Some((
            "argentum config",
            "Configuration review is ready.",
        )),
        "doctor" => Some((
            "argentum doctor",
            "Diagnostics are ready for the selected workspace.",
        )),
        "image-generate" => Some((
            "argentum image \"prompt\"",
            "Image generation requires a prompt and provider approval before execution.",
        )),
        _ => None,
    }
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

#[tauri::command]
async fn test_provider(request: TestProviderRequest) -> Result<TestProviderResponse, String> {
    ensure_allowed("provider API", &request.api, &["openai", "anthropic"])?;

    let defaults = provider_defaults(&request.provider)
        .unwrap_or_else(|| provider_defaults("custom").expect("custom provider defaults"));
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

    if defaults.requires_key && request.api_key.trim().is_empty() {
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
    let api_key = request.api_key.trim();

    if !api_key.is_empty() {
        builder = if request.api == "anthropic" {
            builder
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
        } else {
            builder.bearer_auth(api_key)
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

    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(format!(
            "{} responded, but the API key or account permission was rejected.",
            defaults.label
        ));
    }

    if status.as_u16() == 404 && is_local_endpoint(base_url) {
        return Ok(TestProviderResponse {
            status: "warning".to_string(),
            message: "Local endpoint is reachable, but /models was not found. You can continue in offline guided mode or check your local server.".to_string(),
        });
    }

    Err(format!(
        "{} responded with HTTP {}. Check the endpoint and model, then test again.",
        defaults.label,
        status.as_u16()
    ))
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

#[tauri::command]
fn run_desktop_action(
    request: RunDesktopActionRequest,
) -> Result<RunDesktopActionResponse, String> {
    let _workspace = ensure_safe_workspace(&request.workspace_path)?;
    let Some((command, message)) = desktop_action(&request.action_id) else {
        return Err(format!("Unknown desktop action: {}", request.action_id));
    };

    Ok(RunDesktopActionResponse {
        status: "prepared".to_string(),
        message: message.to_string(),
        command: command.to_string(),
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            save_setup,
            test_provider,
            run_desktop_action,
            desktop_defaults,
            desktop_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argentum");
}
