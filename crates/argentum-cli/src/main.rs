use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use argentum_cli::protocol::{ResponseEnvelope, ServerPayload, PROTOCOL_VERSION};
use argentum_cli::server::serve_jsonl;
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::AppCommand;
use argentum_platform::AppPaths;
use argentum_security::SecretValue;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    init_diagnostics();
    match parse_args(env::args_os().skip(1)) {
        Ok(Arguments::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(Arguments::Version) => {
            print_version();
            ExitCode::SUCCESS
        }
        Ok(arguments) => match run(arguments).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error_value) => {
                eprintln!("argentum-cli: {error_value}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("argentum-cli: {message}");
            eprintln!("Run 'argentum-cli help' for usage.");
            ExitCode::from(2)
        }
    }
}

async fn run(arguments: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    match arguments {
        Arguments::Serve(options) => {
            let host = CommandHost::start(options.host_config()?)?;
            serve_jsonl(
                host,
                BufReader::new(tokio::io::stdin()),
                tokio::io::stdout(),
            )
            .await?;
        }
        Arguments::Run {
            options,
            prompt,
            json,
        } => {
            let host = CommandHost::start(options.host_config()?)?;
            let request_id = "run-1".to_owned();
            let client = host.client();
            let mut events = client.subscribe();
            if json {
                let mut stdout = tokio::io::stdout();
                let sequence = AtomicU64::new(0);
                write_machine_response(
                    &mut stdout,
                    &sequence,
                    Some(request_id.clone()),
                    ServerPayload::CommandAccepted,
                )
                .await?;
                let command = client.dispatch(AppCommand::SubmitTask { prompt });
                tokio::pin!(command);
                loop {
                    tokio::select! {
                        result = &mut command => {
                            let payload = match &result {
                                Ok(()) => ServerPayload::CommandCompleted,
                                Err(error_value) => ServerPayload::command_failed(
                                    "command_failed",
                                    error_value.to_string(),
                                    true,
                                ),
                            };
                            while let Ok(event) = events.try_recv() {
                                write_machine_response(
                                    &mut stdout,
                                    &sequence,
                                    None,
                                    ServerPayload::Event { event },
                                ).await?;
                            }
                            write_machine_response(
                                &mut stdout,
                                &sequence,
                                Some(request_id),
                                payload,
                            ).await?;
                            result?;
                            break;
                        }
                        event = events.recv() => {
                            match event {
                                Ok(event) => write_machine_response(
                                    &mut stdout,
                                    &sequence,
                                    None,
                                    ServerPayload::Event { event },
                                ).await?,
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                    write_machine_response(
                                        &mut stdout,
                                        &sequence,
                                        None,
                                        ServerPayload::error(
                                            "event_lagged",
                                            format!("event subscriber lagged by {skipped} events"),
                                            true,
                                        ),
                                    ).await?;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                            }
                        }
                    }
                }
            } else {
                let command = client.dispatch(AppCommand::SubmitTask { prompt });
                tokio::pin!(command);
                loop {
                    tokio::select! {
                        result = &mut command => {
                            result?;
                            while let Ok(event) = events.try_recv() {
                                print_human_event(event);
                            }
                            break;
                        }
                        event = events.recv() => match event {
                            Ok(event) => print_human_event(event),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                eprintln!("argentum-cli: skipped {skipped} events");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                        }
                    }
                }
            }
        }
        Arguments::Status { options, json } => {
            let host = CommandHost::start(options.host_config()?)?;
            let statuses = host.provider_statuses();
            if json {
                let value = serde_json::json!({
                    "protocol_version": PROTOCOL_VERSION,
                    "workspace": display_workspace(host.workspace_root()),
                    "providers": statuses,
                });
                let mut stdout = tokio::io::stdout();
                stdout.write_all(&serde_json::to_vec(&value)?).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            } else {
                println!("Workspace: {}", display_workspace(host.workspace_root()));
                if statuses.is_empty() {
                    println!("Providers: none configured");
                } else {
                    for status in statuses {
                        println!("{}", format_provider_status_human(&status));
                    }
                }
            }
        }
        Arguments::ProviderProbe {
            options,
            provider_id,
            json,
        } => {
            let host = CommandHost::start(options.host_config()?)?;
            let request_id = "provider-probe-1".to_owned();
            let client = host.client();
            let mut events = client.subscribe();
            if json {
                let mut stdout = tokio::io::stdout();
                let sequence = AtomicU64::new(0);
                write_machine_response(
                    &mut stdout,
                    &sequence,
                    Some(request_id.clone()),
                    ServerPayload::CommandAccepted,
                )
                .await?;
                let result = client
                    .dispatch(AppCommand::ProbeProvider { provider_id })
                    .await;
                while let Ok(event) = events.try_recv() {
                    write_machine_response(
                        &mut stdout,
                        &sequence,
                        None,
                        ServerPayload::Event { event },
                    )
                    .await?;
                }
                let payload = match &result {
                    Ok(()) => ServerPayload::CommandCompleted,
                    Err(error_value) => ServerPayload::command_failed(
                        "command_failed",
                        error_value.to_string(),
                        true,
                    ),
                };
                write_machine_response(&mut stdout, &sequence, Some(request_id), payload).await?;
                result?;
            } else {
                let result = client
                    .dispatch(AppCommand::ProbeProvider { provider_id })
                    .await;
                while let Ok(event) = events.try_recv() {
                    print_human_event(event);
                }
                result?;
            }
        }
        Arguments::ProviderList { options, json } => {
            run_provider_profile_command(options, AppCommand::ListProviderProfiles, json).await?;
        }
        Arguments::ProviderSave {
            options,
            profile,
            json,
        } => {
            run_provider_profile_command(
                options,
                AppCommand::SaveProviderProfile { profile },
                json,
            )
            .await?;
        }
        Arguments::ProviderSelect {
            options,
            provider_id,
            json,
        } => {
            run_provider_profile_command(
                options,
                AppCommand::SelectProviderProfile { provider_id },
                json,
            )
            .await?;
        }
        Arguments::ProviderModels {
            options,
            provider_id,
            json,
        } => {
            run_provider_models_command(options, provider_id, json).await?;
        }
        Arguments::ProviderModelSelect {
            options,
            provider_id,
            model,
            json,
        } => {
            run_provider_profile_command(
                options,
                AppCommand::SelectProviderModel { provider_id, model },
                json,
            )
            .await?;
        }
        Arguments::ProviderCredential {
            options,
            provider_id,
            action,
            json,
        } => {
            run_provider_credential_command(options, provider_id, action, json).await?;
        }
        Arguments::Workspace { action, json } => {
            run_workspace_command(action, json).await?;
        }
        Arguments::Sessions { options, json } => {
            let host = CommandHost::start(options.host_config()?)?;
            print_sessions(host.workspace_snapshot()?, json).await?;
        }
        Arguments::SessionSelect {
            options,
            session_id,
            json,
        } => {
            let host = CommandHost::start(options.host_config()?)?;
            host.client()
                .dispatch(AppCommand::SelectSession { session_id })
                .await?;
            print_sessions(host.workspace_snapshot()?, json).await?;
        }
        Arguments::Goal {
            options,
            action,
            json,
        } => {
            run_goal_command(options, action, json).await?;
        }
        Arguments::Harness {
            options,
            action,
            json,
        } => {
            run_harness_command(options, action, json).await?;
        }
        Arguments::Help | Arguments::Version => {
            unreachable!("help and version exit before command execution")
        }
    }
    Ok(())
}

async fn run_harness_command(
    options: CommonOptions,
    action: HarnessAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = CommandHost::start(options.host_config()?)?;
    match action {
        HarnessAction::Status => {}
        HarnessAction::SelectProfile { profile_id } => {
            host.client()
                .dispatch(AppCommand::SelectHarnessProfile { profile_id })
                .await?;
        }
        HarnessAction::SetSurface { surface, visible } => {
            host.client()
                .dispatch(AppCommand::SetSurfaceVisibility { surface, visible })
                .await?;
        }
        HarnessAction::SelectExecutionProfile { profile_id } => {
            host.client()
                .dispatch(AppCommand::SelectExecutionProfile { profile_id })
                .await?;
        }
        HarnessAction::SetCapability {
            capability_id,
            enabled,
        } => {
            host.client()
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id,
                    enabled,
                })
                .await?;
        }
    }
    print_harness(host.harness_snapshot()?, json);
    Ok(())
}

fn print_harness(snapshot: argentum_domain::HarnessSnapshot, json: bool) {
    if json {
        println!("{}", serde_json::json!({ "harness": snapshot }));
        return;
    }

    println!("Harness profile: {}", snapshot.selected_profile_id);
    println!("Profiles:");
    for profile in snapshot.profiles {
        let state = if profile.selected {
            "selected"
        } else if profile.selectable {
            "available"
        } else {
            "derived"
        };
        println!("  {} [{}]: {}", profile.label, state, profile.detail);
    }
    println!(
        "Execution profile: {}",
        snapshot.selected_execution_profile_id
    );
    println!("Execution profiles:");
    for profile in snapshot.execution_profiles {
        let state = if profile.selected {
            "selected"
        } else if profile.selectable {
            "available"
        } else {
            "derived"
        };
        println!("  {} [{}]: {}", profile.label, state, profile.detail);
    }
    println!("Surfaces:");
    for surface in snapshot.surfaces {
        let state = match surface.availability {
            argentum_domain::HarnessAvailability::Unavailable => "unavailable",
            argentum_domain::HarnessAvailability::Available if surface.visible => "visible",
            argentum_domain::HarnessAvailability::Available => "hidden",
        };
        let detail = if surface.unavailable_reason.is_empty() {
            surface.detail
        } else {
            surface.unavailable_reason
        };
        println!("  {} [{}]: {}", surface.label, state, detail);
    }
    println!("Capabilities:");
    for capability in snapshot.capabilities {
        let state = match capability.availability {
            argentum_domain::HarnessAvailability::Unavailable => "unavailable",
            argentum_domain::HarnessAvailability::Available if !capability.enabled => "disabled",
            argentum_domain::HarnessAvailability::Available => match capability.readiness {
                argentum_domain::HarnessReadiness::Ready => "ready",
                argentum_domain::HarnessReadiness::NeedsConfiguration => "needs configuration",
                argentum_domain::HarnessReadiness::NotVerified => "not verified",
                argentum_domain::HarnessReadiness::Blocked => "blocked",
                argentum_domain::HarnessReadiness::Unavailable => "unavailable",
            },
        };
        let detail = if capability.unavailable_reason.is_empty() {
            capability.detail
        } else {
            capability.unavailable_reason
        };
        println!("  {} [{}]: {}", capability.label, state, detail);
    }
}

async fn run_goal_command(
    options: CommonOptions,
    action: GoalAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = CommandHost::start(options.host_config()?)?;
    let client = host.client();
    match action {
        GoalAction::Status => print_goal(host.goal()?, json).await?,
        action => {
            let command = match action {
                GoalAction::Set {
                    objective,
                    token_budget,
                    tool_budget,
                    time_budget_seconds,
                } => AppCommand::SetGoal {
                    objective,
                    token_budget,
                    tool_budget,
                    time_budget_seconds,
                },
                GoalAction::Pause => AppCommand::PauseGoal,
                GoalAction::Resume => AppCommand::ResumeGoal,
                GoalAction::Clear => AppCommand::ClearGoal,
                GoalAction::Status => unreachable!("status handled above"),
            };
            client.dispatch(command).await?;
            print_goal(host.goal()?, json).await?;
        }
    }
    Ok(())
}

async fn print_goal(
    goal: Option<argentum_domain::Goal>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({ "goal": goal }))?
        );
    } else if let Some(goal) = goal {
        println!("Goal: {}", goal.objective);
        println!("State: {}", goal.lifecycle.label());
        println!("Iteration: {}", goal.iteration);
        println!("Next action: {}", goal.next_action);
        if let Some(budget) = goal.token_budget {
            println!("Tokens: {} / {}", goal.tokens_used, budget);
        }
        if let Some(budget) = goal.tool_budget {
            println!("Tools: {} / {}", goal.tools_used, budget);
        }
        if let Some(budget) = goal.time_budget_seconds {
            println!("Time budget: {} seconds", budget);
        }
        println!("Verification records: {}", goal.verification_history.len());
    } else {
        println!("Goal: none");
    }
    Ok(())
}

async fn run_provider_credential_command(
    options: CommonOptions,
    provider_id: String,
    action: ProviderCredentialAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = CommandHost::start(options.host_config()?)?;
    match action {
        ProviderCredentialAction::Set => {
            let secret = read_secret_from_stdin().await?;
            host.set_provider_credential(&provider_id, secret)?;
            print_credential_status(&provider_id, true, json);
        }
        ProviderCredentialAction::Clear => {
            host.clear_provider_credential(&provider_id)?;
            print_credential_status(&provider_id, false, json);
        }
    }
    Ok(())
}

async fn read_secret_from_stdin() -> Result<SecretValue, Box<dyn std::error::Error>> {
    const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
    let mut bytes = Vec::new();
    tokio::io::stdin()
        .take((MAX_CREDENTIAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > MAX_CREDENTIAL_BYTES {
        return Err("credential input exceeds the supported limit".into());
    }
    let value = String::from_utf8(bytes)?.trim().to_owned();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("credential input must be a non-empty single line".into());
    }
    Ok(SecretValue::new(value))
}

fn print_credential_status(provider_id: &str, configured: bool, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "provider_id": provider_id,
                "configured": configured,
            })
        );
    } else if configured {
        println!("Credential stored securely for {provider_id}.");
    } else {
        println!("Credential removed for {provider_id}.");
    }
}

async fn run_workspace_command(
    action: WorkspaceAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = AppPaths::discover()?;
    match action {
        WorkspaceAction::Status => {
            let workspace = paths.load_workspace()?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "configured": workspace.is_some(),
                        "workspace": workspace.as_ref().map(|path| display_workspace(path)),
                    })
                );
            } else if let Some(workspace) = workspace {
                println!("Workspace: {}", display_workspace(&workspace));
            } else {
                println!("Workspace: not configured");
            }
        }
        WorkspaceAction::Set { path } => {
            let workspace = paths.save_workspace(path)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "configured": true,
                        "workspace": display_workspace(&workspace),
                    })
                );
            } else {
                println!("Workspace saved: {}", display_workspace(&workspace));
            }
        }
    }
    Ok(())
}

async fn run_provider_models_command(
    options: CommonOptions,
    provider_id: String,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = CommandHost::start(options.host_config()?)?;
    let client = host.client();
    let mut events = client.subscribe();
    client
        .dispatch(AppCommand::ListProviderModels { provider_id })
        .await?;
    let mut catalog = None;
    while let Ok(event) = events.try_recv() {
        if let argentum_domain::AppEvent::ProviderModelsSnapshot {
            provider_id,
            models,
            selected_model,
        } = event
        {
            catalog = Some((provider_id, models, selected_model));
        }
    }
    let (provider_id, models, selected_model) =
        catalog.ok_or("provider command completed without a model catalog")?;
    if json {
        let value = serde_json::json!({
            "provider_id": provider_id,
            "selected_model": selected_model,
            "models": models,
        });
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&serde_json::to_vec(&value)?).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    } else {
        for model in models {
            let marker = if model.id == selected_model { "*" } else { " " };
            let context = model
                .context_window_tokens
                .map(|tokens| format!("  context {tokens}"))
                .unwrap_or_default();
            println!("{marker} {}  {}{context}", model.id, model.label);
        }
    }
    Ok(())
}

async fn run_provider_profile_command(
    options: CommonOptions,
    command: AppCommand,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = CommandHost::start(options.host_config()?)?;
    let client = host.client();
    let mut events = client.subscribe();
    client.dispatch(command).await?;
    let mut profiles = None;
    while let Ok(event) = events.try_recv() {
        if let argentum_domain::AppEvent::ProviderProfilesSnapshot { profiles: snapshot } = event {
            profiles = Some(snapshot);
        }
    }
    let profiles = profiles.ok_or("provider command completed without a profile snapshot")?;
    print_provider_profiles(&profiles, json).await
}

async fn print_provider_profiles(
    profiles: &[argentum_domain::ProviderProfile],
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&serde_json::to_vec(profiles)?).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    } else {
        print!("{}", format_provider_profiles_human(profiles));
    }
    Ok(())
}

fn format_provider_profiles_human(profiles: &[argentum_domain::ProviderProfile]) -> String {
    let mut output = String::new();
    for profile in profiles {
        let marker = if profile.selected { "*" } else { " " };
        output.push_str(&format!(
            "{marker} {}  {}  {}  {}  {}\n",
            profile.id,
            profile.label,
            provider_kind_argument(profile.kind),
            profile.model,
            profile.endpoint
        ));
    }
    output
}

fn provider_kind_argument(kind: argentum_domain::ProviderKind) -> &'static str {
    match kind {
        argentum_domain::ProviderKind::LocalLmStudio => "lm-studio",
        argentum_domain::ProviderKind::OpenAiCompatible => "openai-compatible",
        argentum_domain::ProviderKind::Anthropic => "anthropic",
        argentum_domain::ProviderKind::Unknown => "unknown",
    }
}

async fn print_sessions(
    snapshot: argentum_domain::WorkspaceSnapshot,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&serde_json::to_vec(&snapshot)?).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        return Ok(());
    }
    print!("{}", format_sessions_human(&snapshot));
    Ok(())
}

fn format_sessions_human(snapshot: &argentum_domain::WorkspaceSnapshot) -> String {
    let mut output = format!("Project: {}\n", snapshot.project.name);
    for session in &snapshot.sessions {
        let marker = if Some(session.id) == snapshot.active_session_id {
            "*"
        } else {
            " "
        };
        output.push_str(&format!("{marker} {}  {}\n", session.id, session.title));
    }
    output
}

async fn write_machine_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    sequence: &AtomicU64,
    request_id: Option<String>,
    payload: ServerPayload,
) -> std::io::Result<()> {
    let sequence = sequence.fetch_add(1, Ordering::Relaxed) + 1;
    let response = ResponseEnvelope::new(sequence, request_id, payload);
    let mut bytes = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

fn print_human_event(event: argentum_domain::AppEvent) {
    match event {
        argentum_domain::AppEvent::AssistantDelta { text, .. } => print!("{text}"),
        argentum_domain::AppEvent::Error { message, .. } => eprintln!("Error: {message}"),
        argentum_domain::AppEvent::ApprovalRequested(request) => eprintln!(
            "Approval required: {} {} (id: {})",
            request.action, request.target, request.id
        ),
        argentum_domain::AppEvent::RunStatusChanged { lifecycle, .. }
            if lifecycle.is_terminal() =>
        {
            println!();
            eprintln!("Run {}", lifecycle.label().to_lowercase());
        }
        argentum_domain::AppEvent::ProviderStatus(status) => {
            println!("{}", format_provider_status_human(&status));
        }
        _ => {}
    }
}

fn format_provider_status_human(status: &argentum_domain::ProviderStatus) -> String {
    let connection = if status.connected {
        "connected"
    } else {
        "disconnected"
    };
    if status.endpoint.is_empty() {
        format!("{}: {connection}. {}", status.label, status.detail)
    } else {
        format!(
            "{}: {connection}. {} ({})",
            status.label, status.detail, status.endpoint
        )
    }
}

#[derive(Debug)]
enum Arguments {
    Serve(CommonOptions),
    Run {
        options: CommonOptions,
        prompt: String,
        json: bool,
    },
    Status {
        options: CommonOptions,
        json: bool,
    },
    ProviderProbe {
        options: CommonOptions,
        provider_id: String,
        json: bool,
    },
    ProviderList {
        options: CommonOptions,
        json: bool,
    },
    ProviderSave {
        options: CommonOptions,
        profile: argentum_domain::ProviderProfile,
        json: bool,
    },
    ProviderSelect {
        options: CommonOptions,
        provider_id: String,
        json: bool,
    },
    ProviderModels {
        options: CommonOptions,
        provider_id: String,
        json: bool,
    },
    ProviderModelSelect {
        options: CommonOptions,
        provider_id: String,
        model: String,
        json: bool,
    },
    ProviderCredential {
        options: CommonOptions,
        provider_id: String,
        action: ProviderCredentialAction,
        json: bool,
    },
    Workspace {
        action: WorkspaceAction,
        json: bool,
    },
    Sessions {
        options: CommonOptions,
        json: bool,
    },
    SessionSelect {
        options: CommonOptions,
        session_id: argentum_domain::SessionId,
        json: bool,
    },
    Goal {
        options: CommonOptions,
        action: GoalAction,
        json: bool,
    },
    Harness {
        options: CommonOptions,
        action: HarnessAction,
        json: bool,
    },
    Help,
    Version,
}

#[derive(Debug)]
enum GoalAction {
    Status,
    Set {
        objective: String,
        token_budget: Option<u64>,
        tool_budget: Option<u32>,
        time_budget_seconds: Option<u64>,
    },
    Pause,
    Resume,
    Clear,
}

#[derive(Debug)]
enum HarnessAction {
    Status,
    SelectProfile {
        profile_id: String,
    },
    SetSurface {
        surface: argentum_domain::SurfaceId,
        visible: bool,
    },
    SelectExecutionProfile {
        profile_id: String,
    },
    SetCapability {
        capability_id: String,
        enabled: bool,
    },
}

#[derive(Debug)]
enum ProviderCredentialAction {
    Set,
    Clear,
}

#[derive(Debug)]
enum WorkspaceAction {
    Status,
    Set { path: PathBuf },
}

#[derive(Debug, Clone)]
struct CommonOptions {
    workspace: PathBuf,
    database: Option<PathBuf>,
    endpoint: String,
    model: String,
}

impl Default for CommonOptions {
    fn default() -> Self {
        Self {
            workspace: default_workspace(),
            database: None,
            endpoint: argentum_cli::DEFAULT_LM_STUDIO_ENDPOINT.into(),
            model: argentum_cli::DEFAULT_MODEL.into(),
        }
    }
}

fn default_workspace() -> PathBuf {
    AppPaths::discover()
        .ok()
        .and_then(|paths| paths.load_workspace().ok().flatten())
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

impl CommonOptions {
    fn host_config(self) -> Result<HostConfig, Box<dyn std::error::Error>> {
        let mut config = HostConfig::discover(self.workspace)?;
        if let Some(database) = self.database {
            config.database = Some(database);
        }
        config.provider_endpoint = self.endpoint;
        config.model = self.model;
        Ok(config)
    }
}

fn parse_args<I, S>(arguments: I) -> Result<Arguments, String>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString>,
{
    let values = arguments
        .into_iter()
        .map(|value| {
            value
                .into()
                .into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Some(command) = values.first().map(String::as_str) else {
        return Ok(Arguments::Help);
    };
    if matches!(command, "help" | "--help" | "-h") {
        return Ok(Arguments::Help);
    }
    if matches!(command, "version" | "--version" | "-V") {
        return if values.len() == 1 {
            Ok(Arguments::Version)
        } else {
            Err("version does not accept arguments".into())
        };
    }
    let mut options = CommonOptions::default();
    let mut json = false;
    let mut prompt = None;
    let mut session_action = None;
    let mut session_id = None;
    let mut provider_action = None;
    let mut provider_subaction = None;
    let mut provider_id = None;
    let mut provider_label = None;
    let mut provider_kind = None;
    let mut provider_endpoint = None;
    let mut provider_model = None;
    let mut select_saved_provider = false;
    let mut goal_action = None;
    let mut goal_objective = None;
    let mut goal_token_budget = None;
    let mut goal_tool_budget = None;
    let mut goal_time_budget_seconds = None;
    let mut workspace_action = None;
    let mut workspace_path = None;
    let mut harness_action = None;
    let mut harness_target = None;
    let mut harness_visibility = None;
    let mut index = 1;
    while index < values.len() {
        match values[index].as_str() {
            "--workspace" => {
                index += 1;
                options.workspace = PathBuf::from(required_value(&values, index, "--workspace")?);
            }
            "--database" => {
                index += 1;
                options.database =
                    Some(PathBuf::from(required_value(&values, index, "--database")?));
            }
            "--endpoint" => {
                index += 1;
                let value = required_value(&values, index, "--endpoint")?.to_owned();
                if command == "provider" && provider_action.as_deref() == Some("save") {
                    provider_endpoint = Some(value);
                } else {
                    options.endpoint = value;
                }
            }
            "--model" => {
                index += 1;
                let value = required_value(&values, index, "--model")?.to_owned();
                if command == "provider"
                    && matches!(provider_action.as_deref(), Some("save" | "model"))
                {
                    provider_model = Some(value);
                } else {
                    options.model = value;
                }
            }
            "--label" if command == "provider" => {
                index += 1;
                provider_label = Some(required_value(&values, index, "--label")?.to_owned());
            }
            "--kind" if command == "provider" => {
                index += 1;
                provider_kind = Some(parse_provider_kind(required_value(
                    &values, index, "--kind",
                )?)?);
            }
            "--select" if command == "provider" => select_saved_provider = true,
            "--prompt" => {
                index += 1;
                prompt = Some(required_value(&values, index, "--prompt")?.to_owned());
            }
            "--objective" if command == "goal" => {
                index += 1;
                goal_objective = Some(required_value(&values, index, "--objective")?.to_owned());
            }
            "--token-budget" if command == "goal" => {
                index += 1;
                goal_token_budget = Some(parse_u64_option(
                    required_value(&values, index, "--token-budget")?,
                    "--token-budget",
                )?);
            }
            "--tool-budget" if command == "goal" => {
                index += 1;
                goal_tool_budget = Some(parse_u32_option(
                    required_value(&values, index, "--tool-budget")?,
                    "--tool-budget",
                )?);
            }
            "--time-budget" if command == "goal" => {
                index += 1;
                goal_time_budget_seconds = Some(parse_u64_option(
                    required_value(&values, index, "--time-budget")?,
                    "--time-budget",
                )?);
            }
            "--json" => json = true,
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option '{unknown}'"));
            }
            value if command == "run" && prompt.is_none() => prompt = Some(value.to_owned()),
            value if command == "session" && session_action.is_none() => {
                session_action = Some(value.to_owned())
            }
            value
                if command == "session"
                    && session_action.as_deref() == Some("select")
                    && session_id.is_none() =>
            {
                session_id = Some(
                    value
                        .parse()
                        .map_err(|_| "session select requires a valid session ID".to_owned())?,
                )
            }
            value if command == "provider" && provider_action.is_none() => {
                provider_action = Some(value.to_owned())
            }
            value
                if command == "provider"
                    && provider_action.as_deref() == Some("credential")
                    && provider_subaction.is_none() =>
            {
                provider_subaction = Some(value.to_owned())
            }
            value if command == "goal" && goal_action.is_none() => {
                goal_action = Some(value.to_owned())
            }
            value if command == "workspace" && workspace_action.is_none() => {
                workspace_action = Some(value.to_owned())
            }
            value if command == "harness" && harness_action.is_none() => {
                harness_action = Some(value.to_owned())
            }
            value
                if command == "workspace"
                    && workspace_action.as_deref() == Some("set")
                    && workspace_path.is_none() =>
            {
                workspace_path = Some(PathBuf::from(value))
            }
            value
                if command == "harness"
                    && matches!(
                        harness_action.as_deref(),
                        Some("profile" | "surface" | "execution" | "capability")
                    )
                    && harness_target.is_none() =>
            {
                harness_target = Some(value.to_owned())
            }
            value
                if command == "harness"
                    && matches!(harness_action.as_deref(), Some("surface" | "capability"))
                    && harness_target.is_some()
                    && harness_visibility.is_none() =>
            {
                harness_visibility = Some(value.to_owned())
            }
            value
                if command == "goal"
                    && goal_action.as_deref() == Some("set")
                    && goal_objective.is_none() =>
            {
                goal_objective = Some(value.to_owned())
            }
            value
                if command == "provider"
                    && matches!(
                        provider_action.as_deref(),
                        Some("probe" | "save" | "select" | "models" | "model")
                    )
                    && provider_id.is_none() =>
            {
                provider_id = Some(value.to_owned())
            }
            value
                if command == "provider"
                    && provider_action.as_deref() == Some("credential")
                    && matches!(provider_subaction.as_deref(), Some("set" | "clear"))
                    && provider_id.is_none() =>
            {
                provider_id = Some(value.to_owned())
            }
            value => return Err(format!("unexpected argument '{value}'")),
        }
        index += 1;
    }
    match command {
        "serve" if json => Err("serve always uses JSONL; remove --json".into()),
        "serve" if prompt.is_some() => Err("serve does not accept a prompt".into()),
        "serve" => Ok(Arguments::Serve(options)),
        "run" => Ok(Arguments::Run {
            options,
            prompt: prompt.ok_or_else(|| "run requires a prompt".to_owned())?,
            json,
        }),
        "status" if prompt.is_some() => Err("status does not accept a prompt".into()),
        "status" => Ok(Arguments::Status { options, json }),
        "provider" if prompt.is_some() => Err("provider does not accept a prompt".into()),
        "provider" => match provider_action.as_deref() {
            Some("probe") => Ok(Arguments::ProviderProbe {
                options,
                provider_id: provider_id
                    .unwrap_or_else(|| argentum_cli::DEFAULT_PROVIDER_ID.to_owned()),
                json,
            }),
            Some("list") if provider_id.is_none() => Ok(Arguments::ProviderList { options, json }),
            Some("save") => Ok(Arguments::ProviderSave {
                options,
                profile: argentum_domain::ProviderProfile {
                    id: provider_id
                        .ok_or_else(|| "provider save requires PROFILE_ID".to_owned())?,
                    label: provider_label
                        .ok_or_else(|| "provider save requires --label".to_owned())?,
                    kind: provider_kind
                        .ok_or_else(|| "provider save requires --kind".to_owned())?,
                    endpoint: provider_endpoint
                        .ok_or_else(|| "provider save requires --endpoint".to_owned())?,
                    model: provider_model
                        .ok_or_else(|| "provider save requires --model".to_owned())?,
                    selected: select_saved_provider,
                },
                json,
            }),
            Some("select") => Ok(Arguments::ProviderSelect {
                options,
                provider_id: provider_id
                    .ok_or_else(|| "provider select requires PROFILE_ID".to_owned())?,
                json,
            }),
            Some("models") => Ok(Arguments::ProviderModels {
                options,
                provider_id: provider_id
                    .ok_or_else(|| "provider models requires PROFILE_ID".to_owned())?,
                json,
            }),
            Some("model") => Ok(Arguments::ProviderModelSelect {
                options,
                provider_id: provider_id
                    .ok_or_else(|| "provider model requires PROFILE_ID".to_owned())?,
                model: provider_model
                    .ok_or_else(|| "provider model requires --model".to_owned())?,
                json,
            }),
            Some("credential") => match provider_subaction.as_deref() {
                Some("set") => Ok(Arguments::ProviderCredential {
                    options,
                    provider_id: provider_id
                        .ok_or_else(|| "provider credential set requires PROFILE_ID".to_owned())?,
                    action: ProviderCredentialAction::Set,
                    json,
                }),
                Some("clear") => Ok(Arguments::ProviderCredential {
                    options,
                    provider_id: provider_id.ok_or_else(|| {
                        "provider credential clear requires PROFILE_ID".to_owned()
                    })?,
                    action: ProviderCredentialAction::Clear,
                    json,
                }),
                _ => Err("provider credential requires 'set PROFILE_ID' or 'clear PROFILE_ID'".into()),
            },
            _ => Err(
                "provider requires 'list', 'save', 'select', 'models', 'model', 'credential', or 'probe'".into(),
            ),
        },
        "sessions" if prompt.is_some() => Err("sessions does not accept a prompt".into()),
        "sessions" => Ok(Arguments::Sessions { options, json }),
        "session" if session_action.as_deref() != Some("select") => {
            Err("session requires 'select SESSION_ID'".into())
        }
        "session" => Ok(Arguments::SessionSelect {
            options,
            session_id: session_id
                .ok_or_else(|| "session select requires a session ID".to_owned())?,
            json,
        }),
        "goal" if prompt.is_some() => Err("goal does not accept a prompt".into()),
        "goal" => match goal_action.as_deref() {
            Some("status")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none() =>
            {
                Ok(Arguments::Goal {
                    options,
                    action: GoalAction::Status,
                    json,
                })
            }
            Some("set") => Ok(Arguments::Goal {
                options,
                action: GoalAction::Set {
                    objective: goal_objective
                        .ok_or_else(|| "goal set requires an objective".to_owned())?,
                    token_budget: goal_token_budget,
                    tool_budget: goal_tool_budget,
                    time_budget_seconds: goal_time_budget_seconds,
                },
                json,
            }),
            Some("pause")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none() =>
            {
                Ok(Arguments::Goal {
                    options,
                    action: GoalAction::Pause,
                    json,
                })
            }
            Some("resume")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none() =>
            {
                Ok(Arguments::Goal {
                    options,
                    action: GoalAction::Resume,
                    json,
                })
            }
            Some("clear")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none() =>
            {
                Ok(Arguments::Goal {
                    options,
                    action: GoalAction::Clear,
                    json,
                })
            }
            _ => {
                Err("goal requires 'status', 'set OBJECTIVE', 'pause', 'resume', or 'clear'".into())
            }
        },
        "workspace" if prompt.is_some() => Err("workspace does not accept a prompt".into()),
        "workspace" => match workspace_action.as_deref() {
            Some("status") if workspace_path.is_none() => Ok(Arguments::Workspace {
                action: WorkspaceAction::Status,
                json,
            }),
            Some("set") => Ok(Arguments::Workspace {
                action: WorkspaceAction::Set {
                    path: workspace_path
                        .ok_or_else(|| "workspace set requires PATH".to_owned())?,
                },
                json,
            }),
            _ => Err("workspace requires 'status' or 'set PATH'".into()),
        },
        "harness" if prompt.is_some() => Err("harness does not accept a prompt".into()),
        "harness" => match harness_action.as_deref() {
            Some("status") if harness_target.is_none() && harness_visibility.is_none() => {
                Ok(Arguments::Harness {
                    options,
                    action: HarnessAction::Status,
                    json,
                })
            }
            Some("profile") if harness_visibility.is_none() => Ok(Arguments::Harness {
                options,
                action: HarnessAction::SelectProfile {
                    profile_id: harness_target
                        .ok_or_else(|| "harness profile requires PROFILE_ID".to_owned())?,
                },
                json,
            }),
            Some("surface") => {
                let surface = parse_surface_id(
                    harness_target
                        .as_deref()
                        .ok_or_else(|| "harness surface requires SURFACE".to_owned())?,
                )?;
                let visible = match harness_visibility.as_deref() {
                    Some("show") => true,
                    Some("hide") => false,
                    _ => {
                        return Err(
                            "harness surface requires 'show' or 'hide' after SURFACE".into(),
                        )
                    }
                };
                Ok(Arguments::Harness {
                    options,
                    action: HarnessAction::SetSurface { surface, visible },
                    json,
                })
            }
            Some("execution") if harness_visibility.is_none() => Ok(Arguments::Harness {
                options,
                action: HarnessAction::SelectExecutionProfile {
                    profile_id: harness_target
                        .ok_or_else(|| "harness execution requires PROFILE_ID".to_owned())?,
                },
                json,
            }),
            Some("capability") => {
                let capability_id = harness_target
                    .ok_or_else(|| "harness capability requires CAPABILITY_ID".to_owned())?;
                let enabled = match harness_visibility.as_deref() {
                    Some("enable") => true,
                    Some("disable") => false,
                    _ => {
                        return Err(
                            "harness capability requires 'enable' or 'disable' after CAPABILITY_ID"
                                .into(),
                        )
                    }
                };
                Ok(Arguments::Harness {
                    options,
                    action: HarnessAction::SetCapability {
                        capability_id,
                        enabled,
                    },
                    json,
                })
            }
            _ => Err("harness requires 'status', 'profile PROFILE_ID', 'surface SURFACE show|hide', 'execution PROFILE_ID', or 'capability CAPABILITY_ID enable|disable'".into()),
        },
        unknown => Err(format!("unknown command '{unknown}'")),
    }
}

fn required_value<'a>(values: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    values
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_u64_option(value: &str, option: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{option} requires a non-negative integer"))
}

fn parse_u32_option(value: &str, option: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("{option} requires a non-negative integer"))
}

fn parse_provider_kind(value: &str) -> Result<argentum_domain::ProviderKind, String> {
    match value {
        "lm-studio" => Ok(argentum_domain::ProviderKind::LocalLmStudio),
        "openai-compatible" => Ok(argentum_domain::ProviderKind::OpenAiCompatible),
        "anthropic" => Ok(argentum_domain::ProviderKind::Anthropic),
        _ => Err("--kind must be lm-studio, openai-compatible, or anthropic".to_owned()),
    }
}

fn parse_surface_id(value: &str) -> Result<argentum_domain::SurfaceId, String> {
    match value {
        "conversation" => Ok(argentum_domain::SurfaceId::Conversation),
        "plan" => Ok(argentum_domain::SurfaceId::Plan),
        "changes" => Ok(argentum_domain::SurfaceId::Changes),
        "files" => Ok(argentum_domain::SurfaceId::Files),
        "terminal" => Ok(argentum_domain::SurfaceId::Terminal),
        "preview" => Ok(argentum_domain::SurfaceId::Preview),
        "activity" => Ok(argentum_domain::SurfaceId::Activity),
        "approvals" => Ok(argentum_domain::SurfaceId::Approvals),
        _ => Err(
            "SURFACE must be conversation, plan, changes, files, terminal, preview, activity, or approvals"
                .to_owned(),
        ),
    }
}

fn display_workspace(path: &std::path::Path) -> String {
    let raw = path.display().to_string();
    if let Some(rest) = raw.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = raw.strip_prefix("\\\\?\\") {
        rest.to_owned()
    } else {
        raw
    }
}

fn init_diagnostics() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("argentum=warn")),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();
}

fn print_help() {
    println!(
        "Argentum command host\n\n\
Usage:\n  \
argentum-cli serve [OPTIONS]\n  \
argentum-cli run [OPTIONS] --prompt TEXT\n  \
argentum-cli status [OPTIONS]\n  \
argentum-cli provider probe [PROVIDER_ID] [OPTIONS]\n  \
argentum-cli provider list [OPTIONS]\n  \
argentum-cli provider save PROFILE_ID --label LABEL --kind KIND --endpoint URL --model NAME [--select] [OPTIONS]\n  \
argentum-cli provider select PROFILE_ID [OPTIONS]\n  \
  argentum-cli provider models PROFILE_ID [OPTIONS]\n  \
  argentum-cli provider model PROFILE_ID --model NAME [OPTIONS]\n  \
  argentum-cli provider credential set PROFILE_ID < credential.txt\n  \
  argentum-cli provider credential clear PROFILE_ID [OPTIONS]\n  \
  argentum-cli workspace status [OPTIONS]\n  \
  argentum-cli workspace set PATH [OPTIONS]\n  \
  argentum-cli sessions [OPTIONS]\n  \
  argentum-cli session select SESSION_ID [OPTIONS]\n  \
  argentum-cli goal status [OPTIONS]\n  \
  argentum-cli goal set OBJECTIVE [--token-budget N] [--tool-budget N] [--time-budget SECONDS] [OPTIONS]\n  \
  argentum-cli goal pause [OPTIONS]\n  \
  argentum-cli goal resume [OPTIONS]\n  \
  argentum-cli goal clear [OPTIONS]\n  \
  argentum-cli harness status [OPTIONS]\n  \
  argentum-cli harness profile PROFILE_ID [OPTIONS]\n  \
  argentum-cli harness surface SURFACE show|hide [OPTIONS]\n  \
  argentum-cli harness execution PROFILE_ID [OPTIONS]\n  \
  argentum-cli harness capability CAPABILITY_ID enable|disable [OPTIONS]\n\n\
Commands:\n  \
serve   Keep one runtime alive and exchange protocol v1 JSONL on stdin/stdout\n  \
run     Submit one task and stream its output\n  \
status  Show workspace and configured provider information\n  \
  provider Manage and test workspace provider profiles\n  \
  workspace Manage the persisted desktop workspace selection\n  \
  sessions List durable sessions for the current workspace\n  \
  session  Select the active session for the current workspace\n  \
  goal     Manage the active session goal contract\n  \
  harness  Inspect composition, presentation, execution policy, and surfaces\n\n\
Options:\n  \
--workspace PATH   Workspace boundary, defaults to the current directory\n  \
--database PATH    SQLite database path, defaults to Argentum application data\n  \
--endpoint URL     OpenAI-compatible LM Studio endpoint\n  \
--model NAME       Provider model name\n  \
--label LABEL      Display label for provider save\n  \
  --kind KIND        lm-studio, openai-compatible, or anthropic\n  \
  --select           Select a profile when saving it\n  \
  --objective TEXT   Goal objective when using 'goal set'\n  \
  --token-budget N   Maximum model tokens for the goal contract\n  \
  --tool-budget N    Maximum tool calls for the goal contract\n  \
  --time-budget N    Maximum goal duration in seconds\n  \
  provider credential set reads the secret from stdin and never accepts it as an option\n  \
--json             Emit machine-readable output for supported inspection commands\n  \
-V, --version      Print the Argentum CLI version"
    );
}

fn print_version() {
    println!("{}", version_line());
}

fn version_line() -> String {
    format!("argentum-cli {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use argentum_cli::protocol::RequestEnvelope;

    use super::*;

    #[test]
    fn parses_run_and_common_options() {
        let parsed = parse_args([
            "run",
            "--workspace",
            "workspace",
            "--database",
            "state.db",
            "--json",
            "--prompt",
            "inspect",
        ])
        .expect("arguments");
        match parsed {
            Arguments::Run {
                options,
                prompt,
                json,
            } => {
                assert_eq!(options.workspace, PathBuf::from("workspace"));
                assert_eq!(options.database, Some(PathBuf::from("state.db")));
                assert_eq!(prompt, "inspect");
                assert!(json);
            }
            argument => panic!("unexpected arguments: {argument:?}"),
        }
    }

    #[test]
    fn rejects_missing_values_and_unknown_options() {
        assert!(parse_args(["run", "--prompt"]).is_err());
        assert!(parse_args(["status", "--unknown"]).is_err());
    }

    #[test]
    fn parses_version_and_session_commands() {
        assert!(matches!(
            parse_args(["--version"]).expect("version"),
            Arguments::Version
        ));
        assert!(matches!(
            parse_args(["version"]).expect("version subcommand"),
            Arguments::Version
        ));
        assert!(matches!(
            parse_args(["sessions", "--json"]).expect("sessions"),
            Arguments::Sessions { json: true, .. }
        ));
        let session_id = "00000000-0000-0000-0000-000000000071";
        assert!(matches!(
            parse_args(["session", "select", session_id]).expect("session select"),
            Arguments::SessionSelect { session_id: parsed, .. }
                if parsed.to_string() == session_id
        ));
        assert!(parse_args(["version", "extra"]).is_err());
        assert!(parse_args(["session", "select", "invalid"]).is_err());
        assert_eq!(version_line(), "argentum-cli 0.1.0");
    }

    #[test]
    fn parses_goal_lifecycle_commands_and_budgets() {
        assert!(matches!(
            parse_args([
                "goal",
                "set",
                "Ship the bounded slice",
                "--token-budget",
                "4000",
                "--tool-budget",
                "8",
                "--time-budget",
                "1800",
                "--json",
            ])
            .expect("goal set"),
            Arguments::Goal {
                action: GoalAction::Set {
                    objective,
                    token_budget: Some(4000),
                    tool_budget: Some(8),
                    time_budget_seconds: Some(1800),
                },
                json: true,
                ..
            } if objective == "Ship the bounded slice"
        ));
        assert!(matches!(
            parse_args(["goal", "status", "--json"]).expect("goal status"),
            Arguments::Goal {
                action: GoalAction::Status,
                json: true,
                ..
            }
        ));
        assert!(matches!(
            parse_args(["goal", "pause"]).expect("goal pause"),
            Arguments::Goal {
                action: GoalAction::Pause,
                json: false,
                ..
            }
        ));
        assert!(parse_args(["goal", "set"]).is_err());
        assert!(parse_args(["goal", "set", "Objective", "--token-budget", "0"]).is_ok());
        assert!(parse_args(["goal", "pause", "--token-budget", "10"]).is_err());
    }

    #[test]
    fn parses_harness_inspection_presentation_and_execution_commands() {
        assert!(matches!(
            parse_args(["harness", "status", "--json"]).expect("harness status"),
            Arguments::Harness {
                action: HarnessAction::Status,
                json: true,
                ..
            }
        ));
        assert!(matches!(
            parse_args(["harness", "profile", "review"]).expect("harness profile"),
            Arguments::Harness {
                action: HarnessAction::SelectProfile { profile_id },
                ..
            } if profile_id == "review"
        ));
        assert!(matches!(
            parse_args(["harness", "surface", "activity", "show"]).expect("harness surface"),
            Arguments::Harness {
                action: HarnessAction::SetSurface {
                    surface: argentum_domain::SurfaceId::Activity,
                    visible: true,
                },
                ..
            }
        ));
        assert!(matches!(
            parse_args(["harness", "execution", "read-only"])
                .expect("execution profile"),
            Arguments::Harness {
                action: HarnessAction::SelectExecutionProfile { profile_id },
                ..
            } if profile_id == "read-only"
        ));
        assert!(matches!(
            parse_args([
                "harness",
                "capability",
                "tool.write-text",
                "disable",
            ])
            .expect("capability disable"),
            Arguments::Harness {
                action: HarnessAction::SetCapability {
                    capability_id,
                    enabled: false,
                },
                ..
            } if capability_id == "tool.write-text"
        ));
        assert!(parse_args(["harness", "surface", "terminal"]).is_err());
        assert!(parse_args(["harness", "surface", "unknown", "show"]).is_err());
        assert!(parse_args(["harness", "profile"]).is_err());
        assert!(parse_args(["harness", "execution"]).is_err());
        assert!(parse_args(["harness", "capability", "tool.read-text", "show"]).is_err());
    }

    #[test]
    fn parses_provider_probe_with_default_and_explicit_ids() {
        assert!(matches!(
            parse_args(["provider", "probe"]).expect("default provider probe"),
            Arguments::ProviderProbe { provider_id, json: false, .. }
                if provider_id == argentum_cli::DEFAULT_PROVIDER_ID
        ));
        assert!(matches!(
            parse_args([
                "provider",
                "probe",
                "openai-compatible",
                "--endpoint",
                "http://127.0.0.1:8080/v1",
                "--json",
            ])
            .expect("explicit provider probe"),
            Arguments::ProviderProbe { options, provider_id, json: true }
                if provider_id == "openai-compatible"
                    && options.endpoint == "http://127.0.0.1:8080/v1"
        ));
        assert!(parse_args(["provider"]).is_err());
        assert!(parse_args(["provider", "status"]).is_err());
        assert!(parse_args(["provider", "probe", "one", "two"]).is_err());
    }

    #[test]
    fn parses_provider_profile_commands() {
        assert!(matches!(
            parse_args(["provider", "list", "--json"]).expect("provider list"),
            Arguments::ProviderList { json: true, .. }
        ));
        assert!(matches!(
            parse_args([
                "provider",
                "save",
                "local-secondary",
                "--label",
                "Secondary local",
                "--kind",
                "lm-studio",
                "--endpoint",
                "http://127.0.0.1:5678/v1",
                "--model",
                "secondary-model",
                "--select",
                "--json",
            ])
            .expect("provider save"),
            Arguments::ProviderSave { profile, json: true, .. }
                if profile.id == "local-secondary"
                    && profile.label == "Secondary local"
                    && profile.kind == argentum_domain::ProviderKind::LocalLmStudio
                    && profile.selected
        ));
        assert!(matches!(
            parse_args(["provider", "select", "local-secondary"])
                .expect("provider select"),
            Arguments::ProviderSelect { provider_id, .. }
                if provider_id == "local-secondary"
        ));
        assert!(matches!(
            parse_args(["provider", "models", "deepseek", "--json"])
                .expect("provider models"),
            Arguments::ProviderModels { provider_id, json: true, .. }
                if provider_id == "deepseek"
        ));
        assert!(matches!(
            parse_args([
                "provider",
                "model",
                "deepseek",
                "--model",
                "deepseek-chat",
                "--json",
            ])
            .expect("provider model selection"),
            Arguments::ProviderModelSelect { provider_id, model, json: true, .. }
                if provider_id == "deepseek" && model == "deepseek-chat"
        ));
        assert!(parse_args(["provider", "model", "deepseek"]).is_err());
        assert!(parse_args(["provider", "save", "missing-fields"]).is_err());
        assert!(parse_args([
            "provider",
            "save",
            "bad-kind",
            "--label",
            "Bad",
            "--kind",
            "unknown",
            "--endpoint",
            "https://example.test/v1",
            "--model",
            "test",
        ])
        .is_err());
    }

    #[test]
    fn parses_workspace_and_secure_credential_commands() {
        assert!(matches!(
            parse_args(["workspace", "status", "--json"]).expect("workspace status"),
            Arguments::Workspace {
                action: WorkspaceAction::Status,
                json: true,
            }
        ));
        assert!(matches!(
            parse_args(["workspace", "set", r"A:\workspace"]).expect("workspace set"),
            Arguments::Workspace {
                action: WorkspaceAction::Set { path },
                json: false,
            } if path == Path::new(r"A:\workspace")
        ));
        assert!(matches!(
            parse_args(["provider", "credential", "set", "minimax", "--json"])
                .expect("credential set"),
            Arguments::ProviderCredential {
                provider_id,
                action: ProviderCredentialAction::Set,
                json: true,
                ..
            } if provider_id == "minimax"
        ));
        assert!(matches!(
            parse_args(["provider", "credential", "clear", "deepseek"])
                .expect("credential clear"),
            Arguments::ProviderCredential {
                provider_id,
                action: ProviderCredentialAction::Clear,
                json: false,
                ..
            } if provider_id == "deepseek"
        ));
        assert!(parse_args(["provider", "credential", "set", "minimax", "secret"]).is_err());
        assert!(parse_args(["workspace", "set"]).is_err());
    }

    #[test]
    fn formats_selected_provider_profile_for_human_output() {
        let profiles = vec![argentum_domain::ProviderProfile {
            id: "local-secondary".into(),
            label: "Secondary local".into(),
            kind: argentum_domain::ProviderKind::LocalLmStudio,
            endpoint: "http://127.0.0.1:5678/v1/".into(),
            model: "secondary-model".into(),
            selected: true,
        }];

        assert_eq!(
            format_provider_profiles_human(&profiles),
            "* local-secondary  Secondary local  lm-studio  secondary-model  http://127.0.0.1:5678/v1/\n"
        );
    }

    #[test]
    fn formats_truthful_provider_connection_state() {
        let connected = argentum_domain::ProviderStatus {
            profile_id: "lm-studio".into(),
            kind: argentum_domain::ProviderKind::LocalLmStudio,
            label: "LM Studio".into(),
            endpoint: "http://127.0.0.1:1234/v1/".into(),
            connected: true,
            detail: "Reachable; configured model: local/model".into(),
        };
        let disconnected = argentum_domain::ProviderStatus {
            profile_id: "missing".into(),
            kind: argentum_domain::ProviderKind::Unknown,
            label: "missing".into(),
            endpoint: String::new(),
            connected: false,
            detail: "Not configured".into(),
        };

        assert!(format_provider_status_human(&connected).contains("connected"));
        assert!(format_provider_status_human(&connected).contains("127.0.0.1"));
        assert_eq!(
            format_provider_status_human(&disconnected),
            "missing: disconnected. Not configured"
        );
    }

    #[test]
    fn formats_the_active_session_for_human_output() {
        let project_id = "00000000-0000-0000-0000-000000000081"
            .parse()
            .expect("project id");
        let session_id = "00000000-0000-0000-0000-000000000082"
            .parse()
            .expect("session id");
        let timestamp = argentum_domain::now();
        let snapshot = argentum_domain::WorkspaceSnapshot {
            project: argentum_domain::Project {
                id: project_id,
                name: "Workspace".into(),
                workspace_root: PathBuf::from("workspace"),
                created_at: timestamp,
            },
            sessions: vec![argentum_domain::SessionSummary {
                id: session_id,
                title: "Inspect workspace".into(),
                created_at: timestamp,
                updated_at: timestamp,
            }],
            active_session_id: Some(session_id),
        };

        assert_eq!(
            format_sessions_human(&snapshot),
            format!("Project: Workspace\n* {session_id}  Inspect workspace\n")
        );
    }

    #[test]
    fn builds_protocol_request_shape_used_by_serve_clients() {
        let request = RequestEnvelope::command("test", AppCommand::NewSession);
        assert_eq!(request.protocol_version, PROTOCOL_VERSION);
        assert!(request.validate().is_ok());
    }
}
