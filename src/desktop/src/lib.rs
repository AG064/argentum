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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![save_setup])
        .run(tauri::generate_context!())
        .expect("error while running Argentum");
}
