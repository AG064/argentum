use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveSetupRequest {
    workspace_path: String,
    runtime_mode: String,
    llm_provider: String,
    channel_mode: String,
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
    let webchat_enabled = request.channel_mode == "webchat";
    format!(
        "version: \"{version}\"\nllm:\n  default: {llm}\nsecurity:\n  capabilities:\n    defaultProfile: {profile}\n    workspaceRoot: \"{workspace}\"\n    auditPath: ./data/audit/capabilities.log\nfeatures:\n  webchat:\n    enabled: {webchat}\n",
        version = request.version,
        llm = request.llm_provider,
        profile = profile,
        workspace = request.workspace_path,
        webchat = webchat_enabled,
    )
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
    ensure_allowed("runtime mode", &request.runtime_mode, &["desktop", "cli", "service"])?;
    ensure_allowed(
        "LLM provider",
        &request.llm_provider,
        &["openai", "gemini", "anthropic", "local"],
    )?;
    ensure_allowed(
        "channel mode",
        &request.channel_mode,
        &["local-only", "webchat", "telegram"],
    )?;
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

    write_text(&config_path, &render_config(&request))?;
    write_text(
        &secrets_path,
        "# Argentum secrets are stored outside YAML.\n# Provider keys are added by the credential manager.\n",
    )?;

    Ok(SaveSetupResponse {
        status: "setup_saved".to_string(),
        config_path: config_path.display().to_string(),
        secrets_path: secrets_path.display().to_string(),
    })
}

#[tauri::command]
fn run_desktop_action(request: RunDesktopActionRequest) -> Result<RunDesktopActionResponse, String> {
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
        .invoke_handler(tauri::generate_handler![
            save_setup,
            run_desktop_action,
            desktop_defaults,
            desktop_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argentum");
}
