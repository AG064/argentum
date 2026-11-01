use std::env;
use std::future::Future;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "artifacts")]
mod artifact_cli;
#[cfg(feature = "extensions")]
mod extension_browser_cli;
#[cfg(feature = "extensions")]
mod extension_cli;
mod model_picker;

use argentum_cli::onboarding::{
    GuidanceLevel, OnboardingCredentialState, OnboardingSnapshot, OnboardingStep, SetupState,
};
use argentum_cli::protocol::{ResponseEnvelope, ServerPayload, PROTOCOL_VERSION};
use argentum_cli::server::serve_jsonl;
use argentum_cli::tui::prompt_provider_secret;
use argentum_cli::tui::{run_interactive, run_line_session, TuiOptions};
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{
    AppCommand, HarnessCapabilityKind, ProviderCatalogSource, ProviderModel, SessionArchiveFilter,
};
use argentum_platform::AppPaths;
use argentum_providers::{
    download_local_model, install_local_runtime, local_model_offers, local_runtime_bundle_path,
    local_runtime_offers, runtime_bundle_is_valid, LocalRuntimeClient, LocalRuntimeLaunchApproval,
    LocalRuntimeLaunchPlan, PreparedLocalRuntime,
};
use argentum_security::SecretValue;
use argentum_store::Store;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};

fn main() -> ExitCode {
    const CLI_MAIN_STACK_BYTES: usize = 8 * 1024 * 1024;
    match std::thread::Builder::new()
        .name("argentum-cli-main".into())
        .stack_size(CLI_MAIN_STACK_BYTES)
        .spawn(run_main)
    {
        Ok(worker) => match worker.join() {
            Ok(code) => code,
            Err(_) => {
                eprintln!("argentum: CLI runtime thread terminated unexpectedly");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("argentum: failed to start CLI runtime thread: {error}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn run_main() -> ExitCode {
    if env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("artifacts")) {
        #[cfg(feature = "artifacts")]
        {
            let arguments = env::args_os()
                .skip(2)
                .map(|s| {
                    s.into_string()
                        .map_err(|_| "artifact arguments must be UTF-8".into())
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>();
            let result = match arguments {
                Ok(arguments) => artifact_cli::run(arguments).await,
                Err(error) => Err(error),
            };
            return match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("argentum: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        #[cfg(not(feature = "artifacts"))]
        {
            eprintln!("argentum: this build omits optional local artifacts");
            return ExitCode::FAILURE;
        }
    }
    if env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("extensions")) {
        #[cfg(feature = "extensions")]
        {
            let values = env::args_os()
                .skip(2)
                .map(|value| {
                    value
                        .into_string()
                        .map_err(|_| "extension arguments must be UTF-8")
                })
                .collect::<Result<Vec<_>, _>>();
            let result = match values {
                Ok(values) => extension_cli::run(values).await,
                Err(error) => Err(error.into()),
            };
            return match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("argentum: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        #[cfg(not(feature = "extensions"))]
        {
            eprintln!("argentum: this build omits optional extensions; use an extensions-enabled artifact");
            return ExitCode::FAILURE;
        }
    }
    match parse_args(env::args_os().skip(1)) {
        Ok(Arguments::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(Arguments::Version) => {
            print_version();
            ExitCode::SUCCESS
        }
        Ok(arguments) => {
            let run_future = Box::pin(run(arguments));
            match run_future.await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error_value) => {
                    eprintln!("argentum: {error_value}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(message) => {
            eprintln!("argentum: {message}");
            eprintln!("Run 'argentum help' for usage.");
            ExitCode::from(2)
        }
    }
}

async fn start_active_host(config: HostConfig) -> Result<CommandHost, argentum_cli::HostError> {
    CommandHost::start_composed(config).await
}

async fn with_one_shot_host<T, F>(
    config: HostConfig,
    operation: F,
) -> Result<T, Box<dyn std::error::Error>>
where
    F: for<'host> FnOnce(
        &'host CommandHost,
    ) -> Pin<
        Box<dyn Future<Output = Result<T, Box<dyn std::error::Error>>> + 'host>,
    >,
{
    let mut host = start_active_host(config).await?;
    host.disconnect_approval_responder();
    let operation_result = operation(&host).await;
    let shutdown_result = host.shutdown().await;
    finish_host_operation(operation_result, shutdown_result)
}

fn finish_host_operation<T>(
    operation_result: Result<T, Box<dyn std::error::Error>>,
    shutdown_result: Result<(), argentum_cli::HostError>,
) -> Result<T, Box<dyn std::error::Error>> {
    match (operation_result, shutdown_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(operation_error), Ok(())) => Err(operation_error),
        (Ok(_), Err(shutdown_error)) => Err(Box::new(shutdown_error)),
        (Err(operation_error), Err(shutdown_error)) => Err(io::Error::other(format!(
            "command failed: {operation_error}; runtime shutdown failed: {shutdown_error}"
        ))
        .into()),
    }
}

enum FinalResultOutput {
    Result(Box<argentum_domain::final_result::FinalRunResult>),
    Error { code: &'static str, message: String },
}

fn final_result_output_value(output: &FinalResultOutput) -> serde_json::Value {
    match output {
        FinalResultOutput::Result(result) => {
            serde_json::to_value(result).expect("final result is serializable")
        }
        FinalResultOutput::Error { code, message } => serde_json::json!({
            "schema_version": argentum_domain::final_result::FINAL_RESULT_SCHEMA_VERSION,
            "final": false,
            "error": {
                "code": code,
                "message": message,
            }
        }),
    }
}

async fn write_final_result_value(
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > argentum_domain::final_result::FINAL_RESULT_MAX_BYTES {
        return Err("final result exceeds the bounded output size".into());
    }
    let mut stdout = tokio::io::stdout();
    stdout.write_all(&bytes).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

async fn run_final_result_command(
    options: CommonOptions,
    prompt: String,
    answer_schema: Option<String>,
    expected_sha256: Option<argentum_domain::Sha256Digest>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = options.host_config()?;
    let operation = with_one_shot_host(config, move |host| {
        Box::pin(async move {
            let client = host.client();
            let mut events = client.subscribe();
            let command = match answer_schema {
                Some(answer_schema) => AppCommand::SubmitTaskWithAnswerSchema {
                    prompt,
                    answer_schema,
                    expected_sha256,
                },
                None => AppCommand::SubmitTask { prompt },
            };
            let command_result = client.dispatch(command).await;
            let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
            let run_id = emitted.iter().find_map(|event| match event {
                argentum_domain::AppEvent::RunStatusChanged { run_id, .. }
                | argentum_domain::AppEvent::RunError { run_id, .. } => Some(*run_id),
                _ => None,
            });
            let output = match run_id {
                Some(run_id) => match host.final_run_result(run_id)? {
                    Some(result) => FinalResultOutput::Result(Box::new(result)),
                    None => FinalResultOutput::Error {
                        code: "run_result_unavailable",
                        message: "the admitted run has no committed result projection".into(),
                    },
                },
                None if command_result.is_err() => FinalResultOutput::Error {
                    code: "pre_admission_failed",
                    message: command_result
                        .err()
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| "task admission failed".into()),
                },
                None => FinalResultOutput::Error {
                    code: "run_identity_unavailable",
                    message: "the task completed without a committed run identity".into(),
                },
            };
            Ok::<FinalResultOutput, Box<dyn std::error::Error>>(output)
        })
    })
    .await;

    let output = match operation {
        Ok(output) => output,
        Err(error) => FinalResultOutput::Error {
            code: "final_result_unavailable",
            message: error.to_string(),
        },
    };
    let value = final_result_output_value(&output);
    write_final_result_value(&value).await?;
    match output {
        FinalResultOutput::Result(result)
            if result.run_state == argentum_domain::final_result::FinalRunState::Complete =>
        {
            Ok(())
        }
        FinalResultOutput::Result(result) => Err(format!(
            "run settled with {}",
            serde_json::to_string(&result.run_state)?
        )
        .into()),
        FinalResultOutput::Error { message, .. } => Err(message.into()),
    }
}

async fn run_result_lookup_command(
    options: CommonOptions,
    run_id: argentum_domain::RunId,
) -> Result<(), Box<dyn std::error::Error>> {
    let database = options
        .database
        .clone()
        .unwrap_or(AppPaths::discover()?.database);
    let output = match Store::open_existing_read_only(&database) {
        Ok(store) => match store.resolve_existing_workspace(&options.workspace) {
            Ok(resolution) => {
                match store.final_run_result(resolution.snapshot.project.id, run_id)? {
                    Some(result) => FinalResultOutput::Result(Box::new(result)),
                    None => FinalResultOutput::Error {
                        code: "run_not_available_in_project",
                        message: "the run is not available in the requested project".into(),
                    },
                }
            }
            Err(error) => FinalResultOutput::Error {
                code: "workspace_lookup_failed",
                message: error.to_string(),
            },
        },
        Err(error) => FinalResultOutput::Error {
            code: "read_only_lookup_failed",
            message: error.to_string(),
        },
    };
    let value = final_result_output_value(&output);
    write_final_result_value(&value).await?;
    match output {
        FinalResultOutput::Result(result)
            if result.run_state == argentum_domain::final_result::FinalRunState::Complete =>
        {
            Ok(())
        }
        FinalResultOutput::Result(result) => Err(format!(
            "run is not final: {}",
            serde_json::to_string(&result.run_state)?
        )
        .into()),
        FinalResultOutput::Error { message, .. } => Err(message.into()),
    }
}

async fn run_diagnostics_command(
    options: CommonOptions,
    run_id: argentum_domain::RunId,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let database = options
        .database
        .clone()
        .unwrap_or(AppPaths::discover()?.database);
    let store = Store::open_existing_read_only(&database)?;
    let resolution = store.resolve_existing_workspace(&options.workspace)?;
    let report = store
        .run_diagnostics(resolution.snapshot.project.id, None, run_id)?
        .ok_or("the run is not available in the requested project")?;
    if json {
        let output = serde_json::to_vec(&report)?;
        if output.len() > argentum_store::RUN_DIAGNOSTICS_MAX_JSON_BYTES {
            return Err("run diagnostics output exceeds the JSON size limit".into());
        }
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&output).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    } else {
        println!("{}", argentum_store::format_run_diagnostics_human(&report));
    }
    Ok(())
}

async fn run_instruction_preview_command(
    config: HostConfig,
    directory: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let preview = match start_active_host(config.clone()).await {
        Ok(mut host) => {
            host.disconnect_approval_responder();
            let result = host
                .instruction_preview(&directory)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
            finish_host_operation(result, host.shutdown().await)?
        }
        Err(argentum_cli::HostError::Store(argentum_store::StoreError::HostOwnershipLive)) => {
            CommandHost::inspect_existing_instructions(config, &directory)?
        }
        Err(error) => return Err(error.into()),
    };
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

async fn run_context_preview_command(config: HostConfig) -> Result<(), Box<dyn std::error::Error>> {
    let preview = match start_active_host(config.clone()).await {
        Ok(mut host) => {
            host.disconnect_approval_responder();
            let result = host
                .context_preview()
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
            finish_host_operation(result, host.shutdown().await)
        }
        Err(argentum_cli::HostError::Store(argentum_store::StoreError::HostOwnershipLive)) => {
            CommandHost::inspect_existing_context(config)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        }
        Err(error) => Err(error.into()),
    };
    match preview {
        Ok(preview) => println!("{}", serde_json::to_string_pretty(&preview)?),
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"kind":"current_preview_error", "status":"failed", "reason":error.to_string()})
            );
            return Err(error);
        }
    }
    Ok(())
}

async fn run(arguments: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    match arguments {
        Arguments::LocalChat { options, text_only } => {
            if options.non_interactive || !io::stdin().is_terminal() || !io::stdout().is_terminal()
            {
                return Err(setup_error("local chat requires an interactive terminal"));
            }
            let mut config = if options.isolated {
                HostConfig::in_memory(options.workspace.clone())
            } else {
                HostConfig::discover_local_only(options.workspace.clone())?
            };
            if let Some(database) = options.database {
                config.database = Some(database);
            }
            config.model = options.model;
            let host = start_active_host(config).await?;
            if text_only {
                run_local_chat_host(host, false).await?;
            } else {
                run_local_coding_host(host, options.no_color).await?;
            }
        }
        Arguments::TerminalDiagnostic { output } => {
            argentum_cli::tui::run_terminal_diagnostic(&output, || async {
                let mut reader = BufReader::new(tokio::io::stdin());
                let mut writer = tokio::io::stdout();
                read_setup_line(
                    &mut reader,
                    &mut writer,
                    "Synthetic setup reader (abc, then Enter): ",
                )
                .await
                .map_err(|error| io::Error::other(error.to_string()))
            })
            .await?;
        }
        Arguments::Unavailable { message } => return Err(message.into()),
        Arguments::Interactive { options } => {
            let no_color = options.no_color;
            let non_interactive = options.non_interactive;
            let tui_options = TuiOptions {
                no_color,
                force_non_interactive: non_interactive,
            };
            if non_interactive || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                let host = start_active_host(options.clone().host_config()?).await?;
                let mut reader = BufReader::new(tokio::io::stdin());
                let mut writer = tokio::io::stdout();
                let prepared = Box::pin(ensure_onboarding_with_io(
                    host,
                    options,
                    &mut reader,
                    &mut writer,
                    false,
                ))
                .await?;
                if prepared.local_chat {
                    run_local_chat_host(prepared.host, true).await?;
                } else {
                    Box::pin(run_line_session(prepared.host, reader, writer, tui_options)).await?;
                }
            } else {
                let prepared = ensure_onboarding(options.clone()).await?;
                if prepared.local_chat {
                    run_local_chat_host(prepared.host, false).await?;
                } else {
                    let mut reader = BufReader::new(tokio::io::stdin());
                    let mut writer = tokio::io::stdout();
                    maybe_activate_local_runtime(
                        &prepared.host,
                        None,
                        &mut reader,
                        &mut writer,
                        true,
                    )
                    .await?;
                    ensure_local_coding_budget(&prepared.host)?;
                    run_interactive(prepared.host, tui_options).await?;
                }
            }
        }
        Arguments::Setup { options, json } => {
            run_setup_command(options, json).await?;
        }
        Arguments::Serve(options) => {
            let host = start_active_host(options.host_config()?).await?;
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
            goal_id,
            skill,
            final_result,
            answer_schema,
            answer_schema_sha256,
        } => {
            if skill.is_some() && goal_id.is_some() {
                return Err("--skill cannot be combined with --goal".into());
            }
            if final_result {
                if json {
                    return Err(
                        "--final-result cannot be combined with --json; it emits one result envelope"
                            .into(),
                    );
                }
                if goal_id.is_some() || skill.is_some() {
                    return Err("--final-result supports ordinary tasks only".into());
                }
                return run_final_result_command(
                    options,
                    prompt,
                    answer_schema,
                    answer_schema_sha256,
                )
                .await;
            }
            let allow_local_runtime = direct_local_user_interaction(
                options.non_interactive,
                json,
                io::stdin().is_terminal(),
                io::stdout().is_terminal(),
            );
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| Box::pin(async move {
                if allow_local_runtime {
                    let mut reader = BufReader::new(tokio::io::stdin());
                    let mut writer = tokio::io::stdout();
                    maybe_activate_local_runtime(
                        host,
                        None,
                        &mut reader,
                        &mut writer,
                        true,
                    )
                    .await?;
                }
                let request_id = "run-1".to_owned();
                let client = host.client();
                let owner = host.local_handle();
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
                    let command = async { if let Some(goal_id) = goal_id { owner.submit_goal_task(prompt, goal_id).await } else { client.dispatch(task_command(prompt, skill)).await } };
                    tokio::pin!(command);
                    loop {
                        tokio::select! {
                            result = &mut command => {
                                let payload = match &result {
                                    Ok(()) => ServerPayload::CommandCompleted,
                                    Err(error_value) => ServerPayload::host_command_failed(error_value),
                                };
                                while let Ok(event) = events.try_recv() {
                                    write_machine_response(
                                        &mut stdout,
                                        &sequence,
                                        None,
                                        ServerPayload::event(event),
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
                                        ServerPayload::event(event),
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
                    let command = async { if let Some(goal_id) = goal_id { owner.submit_goal_task(prompt, goal_id).await } else { client.dispatch(task_command(prompt, skill)).await } };
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
                                    eprintln!("argentum: skipped {skipped} events");
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                            }
                        }
                    }
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            }))
            .await?;
        }
        Arguments::Result { options, run_id } => {
            run_result_lookup_command(options, run_id).await?;
        }
        Arguments::Diagnostics {
            options,
            run_id,
            json,
        } => {
            run_diagnostics_command(options, run_id, json).await?;
        }
        Arguments::Skills {
            options,
            action,
            json,
        } => {
            let workspace = options.workspace.clone();
            let config = HostConfig::in_memory(workspace);
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    match action {
                        SkillAction::List => {
                            let catalog = host.skill_catalog()?;
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string(&serde_json::json!({
                                        "protocol_version": PROTOCOL_VERSION,
                                        "workspace": display_workspace(host.workspace_root()),
                                        "complete": catalog.complete,
                                        "skills": catalog.entries.iter().map(skill_summary_json).collect::<Vec<_>>(),
                                    }))?
                                );
                            } else {
                                println!("Workspace: {}", display_workspace(host.workspace_root()));
                                println!("Skills: {}", if catalog.complete { "complete" } else { "incomplete" });
                                if catalog.entries.is_empty() {
                                    println!("none");
                                } else {
                                    for summary in catalog.entries {
                                        print_skill_summary(&summary);
                                    }
                                }
                            }
                        }
                        SkillAction::Inspect { name } => {
                            let inspection = host.inspect_skill(&name)?;
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string(&serde_json::json!({
                                        "protocol_version": PROTOCOL_VERSION,
                                        "workspace": display_workspace(host.workspace_root()),
                                        "skill": skill_summary_json(&inspection.summary),
                                        "body": inspection.body,
                                    }))?
                                );
                            } else {
                                print_skill_summary(&inspection.summary);
                                println!("Instructions:\n{}", inspection.body);
                            }
                        }
                    }
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
        }
        Arguments::Status { options, json } => {
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    let statuses = host.provider_statuses();
                    let coding_issue = host.selected_provider_coding_issue()?;
                    let selected = host.provider_profiles()?.into_iter().find(|p| p.selected);
                    let credential_configured = selected.as_ref().filter(|p|
                        argentum_domain::is_supported_hosted_credential_provider_id(&p.id) || p.id=="chatgpt"
                    ).map(|p| host.provider_credential_configured(&p.id));
                    if json {
                        let value = serde_json::json!({
                            "protocol_version": PROTOCOL_VERSION,
                            "workspace": display_workspace(host.workspace_root()),
                            "providers": statuses,
                            "coding_readiness": {
                                "local_configuration_ready": coding_issue.is_none(),
                                "issue": coding_issue,
                                "selected_profile": selected,
                                "credential_configured": credential_configured,
                                "provider_contact": false,
                                "inference_dispatched": false,
                            },
                        });
                        let mut stdout = tokio::io::stdout();
                        stdout.write_all(&serde_json::to_vec(&value)?).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                    } else {
                        println!("Workspace: {}", display_workspace(host.workspace_root()));
                        if let Some(issue) = coding_issue {
                            println!("Coding setup incomplete: {issue}");
                        } else {
                            println!("Local coding configuration ready; provider connection not tested.");
                        }
                        if credential_configured == Some(false) {
                            println!("Selected provider credential is missing; use secure credential entry.");
                        }
                        if statuses.is_empty() {
                            println!("Providers: none configured");
                        } else {
                            for status in statuses {
                                println!("{}", format_provider_status_human(&status));
                            }
                        }
                    }
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
        }
        Arguments::Recovery {
            database,
            action,
            json,
        } => {
            run_recovery_command(database, action, json).await?;
        }
        Arguments::Accounting {
            options,
            action,
            json,
        } => {
            run_accounting_command(options, action, json).await?;
        }
        Arguments::ProviderProbe {
            options,
            provider_id,
            json,
        } => {
            let allow_local_runtime = direct_local_user_interaction(
                options.non_interactive,
                json,
                io::stdin().is_terminal(),
                io::stdout().is_terminal(),
            );
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    if allow_local_runtime {
                        let mut reader = BufReader::new(tokio::io::stdin());
                        let mut writer = tokio::io::stdout();
                        maybe_activate_local_runtime(
                            host,
                            Some(&provider_id),
                            &mut reader,
                            &mut writer,
                            true,
                        )
                        .await?;
                    }
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
                                ServerPayload::event(event),
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
                        write_machine_response(&mut stdout, &sequence, Some(request_id), payload)
                            .await?;
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
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
        }
        Arguments::ProviderList { options, json } => {
            run_provider_profile_command(options, AppCommand::ListProviderProfiles, json).await?;
        }
        Arguments::ProviderBudget {
            options,
            profile_id,
            limits,
        } => {
            with_one_shot_host(options.host_config()?, move |host| {
                Box::pin(async move {
                    let budget = host.declare_model_budget(&profile_id, limits)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "capacity_source": argentum_domain::context_budget::ModelBudget::SOURCE,
                            "budget": budget,
                        }))?
                    );
                    Ok(())
                })
            })
            .await?;
        }
        Arguments::ProviderContext {
            options,
            profile_id,
            context_tokens,
        } => {
            let mut config = if options.isolated {
                HostConfig::in_memory(options.workspace)
            } else {
                HostConfig::discover_local_only(options.workspace)?
            };
            if let Some(database) = options.database {
                config.database = Some(database);
            }
            with_one_shot_host(config,move |host|Box::pin(async move {
                let budget=context_tokens.map(|size|host.configure_local_context(&profile_id,size)).transpose()?;
                let profile=host.provider_profiles()?.into_iter().find(|p|p.id==profile_id).ok_or("local profile is unavailable")?;
                let configured=host.configured_local_context_tokens(&profile.model)?;
                println!("{}",serde_json::to_string_pretty(&serde_json::json!({
                    "profile_id":profile_id,"model":profile.model,"context_tokens":configured,
                    "choices":argentum_providers::SIDECAR_CONTEXT_VARIANTS,
                    "changed":budget.is_some(),"budget":budget,"runtime_started":false,
                    "note":"Applies on the next explicitly approved local launch. Close chat before changing this setting. Memory and speed depend on the model and hardware."
                }))?);
                Ok(())
            })).await?;
        }
        Arguments::ProviderAdd {
            options,
            preset_id,
            json,
        } => {
            run_provider_profile_command(
                options,
                AppCommand::AddProviderPreset { preset_id },
                json,
            )
            .await?;
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
            let allow_local_runtime = direct_local_user_interaction(
                options.non_interactive,
                json,
                io::stdin().is_terminal(),
                io::stdout().is_terminal(),
            );
            run_provider_models_command(options, provider_id, json, allow_local_runtime).await?;
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
        Arguments::ChatGptAuth {
            options,
            action,
            account,
        } => {
            if options.isolated {
                return Err(
                    "ChatGPT sign-in requires the normal secure credential store, not --isolated"
                        .into(),
                );
            }
            if matches!(
                action.as_str(),
                "login" | "login-new" | "logout" | "account"
            ) && (options.non_interactive
                || !io::stdin().is_terminal()
                || !io::stdout().is_terminal())
            {
                return Err(
                    "ChatGPT account changes require a direct interactive owner console".into(),
                );
            }
            let config = options.host_config()?;
            with_one_shot_host(config,move|host|Box::pin(async move {
                let auth=host.chatgpt_auth()?;
                match action.as_str() {
                    "login" | "login-new" => {
                        println!("Opening ChatGPT sign-in in your system browser. Approve there; this command submits no model task.");
                        let id=auth.sign_in(action=="login-new").await?;
                        if !host.provider_profiles()?.iter().any(|p|p.id=="chatgpt") {
                            host.client().dispatch(AppCommand::AddProviderPreset{preset_id:"chatgpt".into()}).await?;
                        }
                        println!("ChatGPT account {} signed in. Use 'provider models chatgpt', then select a model and declare its budget before chat.",&id[..12]);
                    },
                    "logout" => {auth.sign_out().await?;println!("Selected ChatGPT session revoked and local tokens removed. Registration retained.");},
                    "account" => {auth.select(account.as_deref().ok_or("provider account requires an account ID")?)?;println!("ChatGPT account selection changed. Refresh its model catalogue before use.");},
                    "accounts" => {println!("{}",serde_json::to_string(&auth.accounts()?)?);},
                    _=>return Err("unsupported ChatGPT account action".into()),
                }
                Ok(())
            })).await?;
        }
        Arguments::Workspace { action, json } => {
            run_workspace_command(action, json).await?;
        }
        Arguments::Sessions {
            options,
            query,
            filter,
            limit,
            cursor,
            json,
        } => {
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    print_session_list(
                        host.list_sessions(&query, filter, limit, cursor.as_deref())?,
                        json,
                    )
                    .await?;
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
        }
        Arguments::SessionSelect {
            options,
            session_id,
            json,
        } => {
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    host.client()
                        .dispatch(AppCommand::SelectSession { session_id })
                        .await?;
                    print_sessions(host.workspace_snapshot()?, json).await?;
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
        }
        Arguments::SessionOrganize {
            options,
            action,
            json,
        } => {
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    let row = match action {
                        SessionAction::Inspect { session_id } => {
                            host.inspect_session(session_id)?
                        }
                        SessionAction::Title { session_id, title } => {
                            host.client()
                                .dispatch(AppCommand::SetSessionTitle { session_id, title })
                                .await?;
                            host.inspect_session(session_id)?
                        }
                        SessionAction::Pin { session_id, pinned } => {
                            host.client()
                                .dispatch(AppCommand::SetSessionPinned { session_id, pinned })
                                .await?;
                            host.inspect_session(session_id)?
                        }
                        SessionAction::Archive {
                            session_id,
                            archived,
                        } => {
                            host.client()
                                .dispatch(AppCommand::SetSessionArchived {
                                    session_id,
                                    archived,
                                })
                                .await?;
                            host.inspect_session(session_id)?
                        }
                    };
                    print_session_row(row, json).await?;
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
        }
        Arguments::SessionFork {
            options,
            source_session_id,
            title,
            json,
        } => {
            let config = options.host_config()?;
            with_one_shot_host(config, move |host| {
                Box::pin(async move {
                    let preview = host.session_fork_preview(source_session_id, &title)?;
                    if !json {
                        println!("Fork conversation preview");
                        println!("source session: {} ({})", preview.source_session_id, preview.source_title);
                        println!("cutoff message: {}", preview.cutoff.last_message_id);
                        println!("destination title: {}", preview.destination_title);
                        println!("workspace: {}", preview.workspace.display());
                        println!("omitted older sources: {}", preview.omitted_older_sources);
                        println!("{}", preview.warning);
                        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                            return Err("interactive fork confirmation requires a terminal; use --json for an explicit non-interactive command".into());
                        }
                        print!("Create this conversation fork? [y/N]: ");
                        std::io::Write::flush(&mut io::stdout())?;
                        let mut answer = String::new();
                        io::stdin().read_line(&mut answer)?;
                        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                            println!("Fork cancelled. Selection and drafts were not changed.");
                            return Ok::<(), Box<dyn std::error::Error>>(());
                        }
                    }
                    let fork = host.fork_session(
                        source_session_id,
                        &title,
                        Some(preview.cutoff.last_message_id),
                    )?;
                    if json {
                        println!("{}", serde_json::to_string(&fork)?);
                    } else {
                        println!("Created conversation fork {}.", fork.child_session_id);
                        println!(
                            "Selection was not changed. Select it with: argentum session select {} --workspace \"{}\"",
                            fork.child_session_id,
                            host.workspace_snapshot()?.project.workspace_root.display()
                        );
                    }
                    Ok::<(), Box<dyn std::error::Error>>(())
                })
            })
            .await?;
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
        Arguments::Tools { options, json } => {
            run_tool_list_command(options, json).await?;
        }
        Arguments::Instructions { options, directory } => {
            Box::pin(run_instruction_preview_command(
                options.host_config()?,
                directory,
            ))
            .await?;
        }
        Arguments::Context { options } => {
            Box::pin(run_context_preview_command(options.host_config()?)).await?;
        }
        Arguments::Compact {
            options,
            deactivate,
            goal_id,
        } => {
            with_one_shot_host(options.host_config()?, move |host| {
                Box::pin(async move {
                    if !deactivate { eprintln!("Context compaction uses the selected model and may consume usage; tools remain disabled. Accounting scope: {}.", goal_id.map_or_else(|| "owner-initiated maintenance outside goal quotas".into(), |id| format!("goal {id}"))); }
                    let outcome = if let Some(goal_id) = goal_id {
                        host.local_handle().compact_goal_context(host.workspace_snapshot()?.active_session_id.ok_or("no active session")?, argentum_domain::RunId::new_v4(), goal_id).await
                    } else { host.compact_context(deactivate).await };
                    match outcome {
                        Ok(result) => {
                            let mut output = serde_json::to_value(&result)?;
                            output["goal_id"] = serde_json::to_value(goal_id)?;
                            output["accounting_scope"] = serde_json::json!(if deactivate { "provider_free" } else if goal_id.is_some() { "explicit_goal" } else { "owner_maintenance_outside_goal_quota" });
                            println!("{}", serde_json::to_string_pretty(&output)?);
                            if result.status == argentum_domain::context_summary::CompactionStatus::Failed {
                                return Err(result.reason.into());
                            }
                        }
                        Err(error) => {
                            let failure = serde_json::to_value(ServerPayload::host_command_failed(&error))?;
                            println!("{}", serde_json::json!({"status":"failed", "reason":error.to_string(),
                                "code":failure["code"],
                                "observed_usage":null, "checkpoint_activation":"not_committed"}));
                            return Err(error.into());
                        }
                    }
                    Ok(())
                })
            }).await?;
        }
        Arguments::Permissions {
            options,
            action,
            json,
        } => {
            run_permissions_command(options, action, json).await?;
        }
        Arguments::Verification {
            options,
            action,
            json,
        } => {
            run_verification_command(options, action, json).await?;
        }
        Arguments::Changes {
            options,
            action,
            json,
        } => {
            run_changes_command(options, action, json).await?;
        }
        Arguments::Help | Arguments::Version => {
            unreachable!("help and version exit before command execution")
        }
    }
    Ok(())
}

const MAX_ONBOARDING_INPUT_BYTES: usize = 4 * 1024;
const MAX_ONBOARDING_ATTEMPTS: usize = 8;

async fn run_local_chat_host(
    mut host: CommandHost,
    non_interactive: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result: Result<(), Box<dyn std::error::Error>> = async {
        if non_interactive || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(setup_error("local-text requires a terminal; it is explicitly text-only with no coding tools"));
        }
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut writer = tokio::io::stdout();
        write_setup_text(&mut writer, "Local llama.cpp text chat. No coding tools or task execution. Conversation stays in memory for this session.\n").await?;
        if !maybe_activate_local_runtime(&host, Some("llama-cpp"), &mut reader, &mut writer, true).await? {
            return Err(setup_error("local runtime was not activated; no inference was started"));
        }
        argentum_cli::tui::run_local_text_session(&mut host).await?;
        Ok(())
    }.await;
    finish_host_operation(result, host.shutdown().await)
}

async fn run_local_coding_host(
    mut host: CommandHost,
    no_color: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let prepared: Result<(), Box<dyn std::error::Error>> = async {
        host.local_handle().dispatch(AppCommand::SelectProviderProfile {provider_id:"llama-cpp".into()}).await?;
        if !ensure_local_coding_budget(&host)? {
            return Err(setup_error("local coding backend is not configured; local-text is the explicit no-tools mode"));
        }
        let mut reader=BufReader::new(tokio::io::stdin());
        let mut writer=tokio::io::stdout();
        write_setup_text(&mut writer,"Local coding chat. Tools use your persisted access policy; model/template validation occurs before requests.\n").await?;
        if !maybe_activate_local_runtime(&host,Some("llama-cpp"),&mut reader,&mut writer,true).await? {
            return Err(setup_error("local runtime was not activated; no inference was started"));
        }
        Ok(())
    }.await;
    if let Err(error) = prepared {
        return finish_host_operation(Err(error), host.shutdown().await);
    }
    run_interactive(
        host,
        TuiOptions {
            no_color,
            force_non_interactive: false,
        },
    )
    .await?;
    Ok(())
}

fn can_download_local_model(options: &CommonOptions) -> bool {
    direct_local_user_interaction(
        options.non_interactive,
        false,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )
}

fn direct_local_user_interaction(
    non_interactive: bool,
    machine_output: bool,
    stdin_terminal: bool,
    stdout_terminal: bool,
) -> bool {
    !non_interactive && !machine_output && stdin_terminal && stdout_terminal
}

struct SetupResult {
    host: CommandHost,
    snapshot: OnboardingSnapshot,
    local_chat: bool,
}

async fn ensure_onboarding(
    options: CommonOptions,
) -> Result<SetupResult, Box<dyn std::error::Error>> {
    let host = start_active_host(options.clone().host_config()?).await?;
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::stdout();
    ensure_onboarding_with_io(host, options, &mut reader, &mut writer, true).await
}

async fn ensure_onboarding_with_io<R, W>(
    host: CommandHost,
    options: CommonOptions,
    reader: &mut R,
    writer: &mut W,
    local_authority_observed: bool,
) -> Result<SetupResult, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    if host.onboarding_snapshot()?.complete {
        let local_chat = selected_profile_needs_local_text(&host)?;
        return Ok(SetupResult {
            snapshot: host.onboarding_snapshot()?,
            host,
            local_chat,
        });
    }
    let allow_local_model_download = can_download_local_model(&options);
    Box::pin(run_setup_flow(
        host,
        options,
        reader,
        writer,
        false,
        allow_local_model_download,
        local_authority_observed,
    ))
    .await
}

async fn run_setup_command(
    options: CommonOptions,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = start_active_host(options.clone().host_config()?).await?;
    let mut reader = BufReader::new(tokio::io::stdin());
    let allow_local_model_download = can_download_local_model(&options) && !json;
    let local_authority_observed = direct_local_user_interaction(
        options.non_interactive,
        json,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    );
    if json {
        let mut prompt_writer = tokio::io::stderr();
        let SetupResult {
            mut host, snapshot, ..
        } = Box::pin(run_setup_flow(
            host,
            options,
            &mut reader,
            &mut prompt_writer,
            true,
            allow_local_model_download,
            local_authority_observed,
        ))
        .await?;
        let output_result: Result<(), Box<dyn std::error::Error>> = async {
            let mut output = tokio::io::stdout();
            output
                .write_all(serde_json::to_string(&snapshot)?.as_bytes())
                .await?;
            output.write_all(b"\n").await?;
            output.flush().await?;
            Ok(())
        }
        .await;
        finish_host_operation(output_result, host.shutdown().await)
    } else {
        let mut writer = tokio::io::stdout();
        let SetupResult {
            mut host,
            local_chat,
            ..
        } = Box::pin(run_setup_flow(
            host,
            options,
            &mut reader,
            &mut writer,
            true,
            allow_local_model_download,
            local_authority_observed,
        ))
        .await?;
        if local_chat {
            return run_local_chat_host(host, !local_authority_observed).await;
        }
        let output_result = write_setup_text(&mut writer, "Setup complete.\n")
            .await
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
        finish_host_operation(output_result, host.shutdown().await)
    }
}

async fn run_setup_flow<R, W>(
    mut host: CommandHost,
    options: CommonOptions,
    reader: &mut R,
    writer: &mut W,
    reconfigure_complete: bool,
    allow_local_model_download: bool,
    local_authority_observed: bool,
) -> Result<SetupResult, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let result = run_setup_flow_inner(
        &mut host,
        options,
        reader,
        writer,
        reconfigure_complete,
        allow_local_model_download,
        local_authority_observed,
    )
    .await;
    match result {
        Ok(result) => Ok(SetupResult {
            host: result.replacement.unwrap_or(host),
            snapshot: result.snapshot,
            local_chat: result.local_chat,
        }),
        Err(error) => finish_host_operation(Err(error), host.shutdown().await),
    }
}

struct SetupFlowOutcome {
    snapshot: OnboardingSnapshot,
    replacement: Option<CommandHost>,
    local_chat: bool,
}

async fn run_setup_flow_inner<R, W>(
    host: &mut CommandHost,
    options: CommonOptions,
    reader: &mut R,
    writer: &mut W,
    reconfigure_complete: bool,
    allow_local_model_download: bool,
    local_authority_observed: bool,
) -> Result<SetupFlowOutcome, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut snapshot = host.onboarding_snapshot()?;
    if reconfigure_complete && snapshot.complete {
        snapshot = dispatch_onboarding(host, AppCommand::BeginOnboarding, false).await?;
    }
    let mut state = SetupState::new(snapshot);

    loop {
        let command = match state.step() {
            OnboardingStep::Guidance => prompt_guidance(reader, writer).await?,
            OnboardingStep::Provider => match prompt_provider(
                host,
                &state,
                reader,
                writer,
                local_authority_observed && !options.isolated,
            )
            .await?
            {
                ProviderChoice::Coding(command) => command,
                ProviderChoice::LocalChat => {
                    return Ok(SetupFlowOutcome {
                        snapshot: state.snapshot().clone(),
                        replacement: None,
                        local_chat: true,
                    })
                }
            },
            OnboardingStep::Model => {
                prompt_model(host, &state, reader, writer, allow_local_model_download).await?
            }
            OnboardingStep::Workspace => prompt_workspace(host, &state, reader, writer).await?,
            OnboardingStep::Permission => prompt_permission(host, &state, reader, writer).await?,
            OnboardingStep::Review => {
                ensure_setup_model_budget(host, reader, writer).await?;
                show_review(
                    host,
                    &state,
                    writer,
                    options
                        .isolated
                        .then_some(options.database.as_deref())
                        .flatten(),
                )
                .await?;
                if !prompt_confirmation(reader, writer).await? {
                    return Err(setup_error(
                        "onboarding remains incomplete; run 'argentum setup' to resume",
                    ));
                }
                AppCommand::CompleteOnboarding
            }
            OnboardingStep::Complete => break,
        };
        let observed_permission = local_authority_observed
            && matches!(command, AppCommand::SetOnboardingPermission { .. });
        snapshot = dispatch_onboarding(host, command, observed_permission).await?;
        state = SetupState::new(snapshot);
    }

    let snapshot = state.snapshot().clone();
    let target_workspace = snapshot
        .workspace
        .clone()
        .ok_or_else(|| setup_error("onboarding completed without a workspace"))?;
    let provider_id = snapshot
        .provider_id
        .clone()
        .ok_or_else(|| setup_error("onboarding completed without a provider"))?;
    let selected_profile = host
        .provider_profiles()?
        .into_iter()
        .find(|profile| profile.id == provider_id)
        .ok_or_else(|| setup_error("onboarding completed without a provider profile"))?;

    let current_workspace = host.workspace_root().to_path_buf();
    if current_workspace != target_workspace {
        let budget = host.selected_model_budget()?;
        let mut target_config = options.clone().host_config()?;
        target_config.workspace = target_workspace.clone();
        // One writable host owns a database at a time, including setup moves.
        host.shutdown().await?;
        let mut target_host = start_active_host(target_config).await?;
        let target_result: Result<OnboardingSnapshot, Box<dyn std::error::Error>> = async {
            dispatch_onboarding(&target_host, AppCommand::BeginOnboarding, false).await?;
            let mut selected_profile = selected_profile;
            selected_profile.selected = true;
            dispatch_onboarding(
                &target_host,
                AppCommand::SaveProviderProfile {
                    profile: selected_profile,
                },
                false,
            )
            .await
            .map(|_| ())?;
            if let Some(budget) = budget {
                target_host.declare_model_budget(
                    &budget.profile_id,
                    [
                        budget.usable_context_tokens,
                        budget
                            .input_limit_tokens
                            .unwrap_or(budget.usable_context_tokens),
                        u64::from(budget.maximum_output_tokens),
                        u64::from(budget.requested_output_tokens),
                        budget.uncertainty_tokens,
                    ],
                )?;
            }
            let target_snapshot =
                apply_completed_setup(&target_host, &snapshot, local_authority_observed).await?;
            if !options.isolated {
                AppPaths::discover()?.save_workspace(&target_workspace)?;
            }
            Ok(target_snapshot)
        }
        .await;
        let target_snapshot = match target_result {
            Ok(target_snapshot) => target_snapshot,
            Err(error) => {
                return finish_host_operation::<SetupFlowOutcome>(
                    Err(error),
                    target_host.shutdown().await,
                );
            }
        };
        if let Err(error) = host.shutdown().await {
            return finish_host_operation::<SetupFlowOutcome>(
                Err(Box::new(error)),
                target_host.shutdown().await,
            );
        }
        return Ok(SetupFlowOutcome {
            snapshot: target_snapshot,
            replacement: Some(target_host),
            local_chat: false,
        });
    }

    if !options.isolated {
        AppPaths::discover()?.save_workspace(&target_workspace)?;
    }
    Ok(SetupFlowOutcome {
        snapshot,
        replacement: None,
        local_chat: false,
    })
}

async fn apply_completed_setup(
    host: &CommandHost,
    snapshot: &OnboardingSnapshot,
    local_authority_observed: bool,
) -> Result<OnboardingSnapshot, Box<dyn std::error::Error>> {
    let guidance = snapshot
        .guidance
        .ok_or_else(|| setup_error("onboarding completed without a guidance level"))?;
    let provider_id = snapshot
        .provider_id
        .clone()
        .ok_or_else(|| setup_error("onboarding completed without a provider"))?;
    let model = snapshot
        .model
        .clone()
        .ok_or_else(|| setup_error("onboarding completed without a model"))?;
    let workspace = snapshot
        .workspace
        .clone()
        .ok_or_else(|| setup_error("onboarding completed without a workspace"))?;
    let permission_profile_id = snapshot
        .permission_profile_id
        .clone()
        .ok_or_else(|| setup_error("onboarding completed without a permission profile"))?;
    let provider = host
        .provider_profiles()?
        .into_iter()
        .find(|profile| profile.id == provider_id)
        .ok_or_else(|| setup_error("onboarding completed without a provider profile"))?;
    let mut capability_profile = provider;
    capability_profile.model = model.clone();
    if let Some(issue) = coding_capability_issue(host, &capability_profile)? {
        return Err(setup_error(&format!(
            "selected provider/model cannot run the coding workflow: {issue}; setup remains incomplete"
        )));
    }

    dispatch_onboarding(host, AppCommand::SetOnboardingGuidance { guidance }, false).await?;
    dispatch_onboarding(
        host,
        AppCommand::SetOnboardingProvider {
            provider_id: provider_id.clone(),
        },
        false,
    )
    .await?;
    dispatch_onboarding(
        host,
        AppCommand::SetOnboardingModel { provider_id, model },
        false,
    )
    .await?;
    dispatch_onboarding(
        host,
        AppCommand::SetOnboardingWorkspace { workspace },
        false,
    )
    .await?;
    dispatch_onboarding(
        host,
        AppCommand::SetOnboardingPermission {
            profile_id: permission_profile_id,
        },
        local_authority_observed,
    )
    .await?;
    dispatch_onboarding(host, AppCommand::CompleteOnboarding, false).await
}

async fn dispatch_onboarding(
    host: &CommandHost,
    command: AppCommand,
    local_authority_observed: bool,
) -> Result<OnboardingSnapshot, Box<dyn std::error::Error>> {
    if local_authority_observed {
        host.local_handle().dispatch(command).await?;
    } else {
        host.client().dispatch(command).await?;
    }
    Ok(host.onboarding_snapshot()?)
}

async fn prompt_guidance<R, W>(
    reader: &mut R,
    writer: &mut W,
) -> Result<AppCommand, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_setup_text(
        writer,
        "Would you like guided setup?\n1) Yes, guide me\n2) No, use expert setup\n",
    )
    .await?;
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(reader, writer, "Choice [1/2]: ").await?;
        let guidance = match value.to_ascii_lowercase().as_str() {
            "1" | "yes" | "guided" => GuidanceLevel::Guided,
            "2" | "no" | "expert" => GuidanceLevel::Expert,
            _ => {
                write_setup_text(writer, "Choose 1 for guided setup or 2 for expert setup.\n")
                    .await?;
                continue;
            }
        };
        return Ok(AppCommand::SetOnboardingGuidance { guidance });
    }
    Err(setup_error("too many invalid onboarding choices"))
}

enum ProviderChoice {
    Coding(AppCommand),
    LocalChat,
}

fn selected_profile_needs_local_text(
    host: &CommandHost,
) -> Result<bool, Box<dyn std::error::Error>> {
    let selected = host
        .provider_profiles()?
        .into_iter()
        .find(|profile| profile.selected);
    Ok(match selected {
        Some(profile) if profile.kind == argentum_domain::ProviderKind::LocalLlamaCpp => {
            !host.provider_capabilities(&profile)?.supports_coding()
        }
        _ => false,
    })
}

fn ensure_local_coding_budget(host: &CommandHost) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(profile) = host.provider_profiles()?.into_iter().find(|p| p.selected) else {
        return Ok(false);
    };
    let caps = host.provider_capabilities(&profile)?;
    if caps.identity.local_backend_id != Some("llama.cpp.sidecar") || !caps.supports_coding() {
        return Ok(false);
    }
    if host.selected_model_budget()?.is_none() {
        let context = u64::from(argentum_providers::SIDECAR_CONTEXT_TOKENS);
        host.declare_model_budget(&profile.id, [context, context, context - 1, 0, 128])?;
    }
    Ok(true)
}

async fn prompt_provider<R, W>(
    host: &CommandHost,
    state: &SetupState,
    reader: &mut R,
    writer: &mut W,
    allow_credentials: bool,
) -> Result<ProviderChoice, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    prompt_provider_with_credentials(
        host,
        state,
        reader,
        writer,
        allow_credentials,
        &mut prompt_provider_secret,
    )
    .await
}

async fn prompt_provider_with_credentials<R, W, F>(
    host: &CommandHost,
    state: &SetupState,
    reader: &mut R,
    writer: &mut W,
    allow_credentials: bool,
    prompt_secret: &mut F,
) -> Result<ProviderChoice, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut(&str) -> io::Result<SecretValue>,
{
    let mut profiles = host.provider_profiles()?;
    if profiles.is_empty() {
        return Err(setup_error("no provider profiles are available"));
    }
    if state.snapshot().guidance == Some(GuidanceLevel::Guided) {
        write_setup_text(
            writer,
            "Choose a provider below. Coding availability is shown for each profile. Type /cancel to leave setup at any prompt.\n",
        )
        .await?;
    }
    for profile in &profiles {
        let capability = if profile.kind == argentum_domain::ProviderKind::LocalLlamaCpp
            && coding_capability_issue(host, profile)?.is_some()
        {
            "local text chat; no coding tools; runtime checked at launch".to_owned()
        } else {
            coding_capability_issue(host, profile)?.map_or_else(
                || "coding supported".to_owned(),
                |issue| format!("coding unavailable: {issue}"),
            )
        };
        write_setup_text(
            writer,
            &format!("  {}  {}  [{capability}]\n", profile.id, profile.label),
        )
        .await?;
    }
    let local_action = if profiles.iter().any(|profile| {
        profile.id == "llama-cpp"
            && host
                .provider_capabilities(profile)
                .is_ok_and(|capabilities| capabilities.supports_coding())
    }) {
        "Use llama.cpp coding (tools enabled; runtime checked at launch)"
    } else {
        "Start llama.cpp text-only chat (no coding tools)"
    };
    write_setup_text(writer, &format!("  minimax   Add or configure MiniMax coding (API key required)\n  deepseek  Add or configure DeepSeek coding (API key required)\n  chatgpt   ChatGPT coding through browser sign-in (no API key)\n  server    Connect an OpenAI-compatible HTTP server\n  local     {local_action}\n  /cancel   Exit setup and keep saved settings\n")).await?;
    let default_id = profiles
        .iter()
        .find(|profile| {
            profile.selected
                && (profile.kind == argentum_domain::ProviderKind::LocalLlamaCpp
                    || host
                        .provider_capabilities(profile)
                        .is_ok_and(|c| c.supports_coding()))
        })
        .map(|profile| profile.id.clone())
        .unwrap_or_else(|| "minimax".into());
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(
            reader,
            writer,
            &format!("Provider or action [{default_id}]: "),
        )
        .await?;
        let mut provider_id = if value.is_empty() {
            default_id.clone()
        } else {
            value
        };
        if matches!(provider_id.as_str(), "local" | "llama-cpp") {
            provider_id = "llama-cpp".into();
            let coding = profiles
                .iter()
                .find(|p| p.id == provider_id)
                .is_some_and(|p| {
                    host.provider_capabilities(p)
                        .is_ok_and(|c| c.supports_coding())
                });
            if !coding {
                let answer = read_setup_line(reader, writer, "Start llama.cpp in text-only mode? It cannot use coding tools or claim file changes. [y/N]: ").await?;
                if matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes") {
                    host.client()
                        .dispatch(AppCommand::SelectProviderProfile {
                            provider_id: "llama-cpp".into(),
                        })
                        .await?;
                    return Ok(ProviderChoice::LocalChat);
                }
                write_setup_text(
                    writer,
                    "Local chat not started. Choose minimax, deepseek, server, or /cancel.\n",
                )
                .await?;
                continue;
            }
        }
        if provider_id == "server" {
            provider_id = prompt_compatible_server(host, reader, writer).await?;
            profiles = host.provider_profiles()?;
        } else if matches!(provider_id.as_str(), "minimax" | "deepseek" | "chatgpt")
            && !profiles.iter().any(|p| p.id == provider_id)
        {
            host.client()
                .dispatch(AppCommand::AddProviderPreset {
                    preset_id: provider_id.clone(),
                })
                .await?;
            profiles = host.provider_profiles()?;
        }
        if let Some(profile) = profiles.iter().find(|profile| profile.id == provider_id) {
            if profile.id == "chatgpt" && !host.provider_credential_configured("chatgpt") {
                write_setup_text(writer,"ChatGPT uses browser sign-in, not an API key. Cancel setup and run 'argentum provider login chatgpt', then return here. No model request was sent.\n").await?;
                continue;
            }
            if let Some(issue) = coding_capability_issue(host, profile)? {
                write_setup_text(
                    writer,
                    &format!(
                        "That profile cannot run the coding workflow: {issue}\nChoose a profile marked coding supported. No inference was started and setup remains incomplete.\n"
                    ),
                )
                .await?;
                continue;
            }
            if argentum_domain::is_supported_hosted_credential_provider_id(&profile.id)
                && !host.provider_credential_configured(&profile.id)
            {
                if allow_credentials {
                    let choice = read_setup_line(
                        reader,
                        writer,
                        "An API key is required. Enter it here with input hidden? [y/N]: ",
                    )
                    .await?;
                    if matches!(choice.to_ascii_lowercase().as_str(), "y" | "yes") {
                        match prompt_secret(&profile.id) {
                            Ok(secret) => match host.set_provider_credential(&profile.id, secret) {
                                Ok(()) => write_setup_text(writer, "Key saved securely. Continuing setup.\n").await?,
                                Err(_) => write_setup_text(writer, "Secure storage could not save the key. Choose another provider or cancel setup.\n").await?,
                            },
                            Err(_) => write_setup_text(writer, "Key entry cancelled or invalid. No key was saved; setup is still open.\n").await?,
                        }
                    }
                }
                if !host.provider_credential_configured(&profile.id) {
                    write_setup_text(writer, &format!("{} is saved, but needs its own API key. Choose local/server, or /cancel and run 'argentum provider credential set {}'. No model request was sent.\n", profile.label, profile.id)).await?;
                    continue;
                }
            }
            return Ok(ProviderChoice::Coding(AppCommand::SetOnboardingProvider {
                provider_id,
            }));
        }
        write_setup_text(writer, "That provider profile is not available.\n").await?;
    }
    Err(setup_error("too many invalid provider choices"))
}

async fn prompt_compatible_server<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    host: &CommandHost,
    reader: &mut R,
    writer: &mut W,
) -> Result<String, Box<dyn std::error::Error>> {
    write_setup_text(writer, "Connect an existing HTTP server using the implemented chat-completions tool adapter. Do not enter API keys in URLs or model names.\n").await?;
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let id = read_setup_line(reader, writer, "New profile ID [local-server]: ").await?;
        let id = if id.is_empty() {
            "local-server".to_owned()
        } else {
            id
        };
        if host.provider_profiles()?.iter().any(|p| p.id == id)
            || argentum_providers::provider_preset(&id).is_some()
        {
            write_setup_text(
                writer,
                "That profile ID is already used or reserved; choose a new ID.\n",
            )
            .await?;
            continue;
        }
        let endpoint =
            read_setup_line(reader, writer, "Server API base URL (including /v1): ").await?;
        let model = read_setup_line(reader, writer, "Model ID served by that endpoint: ").await?;
        let profile = argentum_domain::ProviderProfile {
            id: id.clone(),
            label: id.clone(),
            kind: argentum_domain::ProviderKind::OpenAiCompatible,
            endpoint,
            model,
            selected: false,
        };
        match argentum_providers::normalize_provider_profile(profile) {
            Ok(profile) => {
                host.client()
                    .dispatch(AppCommand::SaveProviderProfile { profile })
                    .await?;
                return Ok(id);
            }
            Err(error) => {
                write_setup_text(writer, &format!("Profile not saved: {error}\n")).await?
            }
        }
    }
    Err(setup_error("too many invalid server profiles"))
}

async fn ensure_setup_model_budget<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    host: &CommandHost,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), Box<dyn std::error::Error>> {
    if ensure_local_coding_budget(host)? {
        let profile = host
            .provider_profiles()?
            .into_iter()
            .find(|p| p.selected)
            .ok_or("selected profile unavailable")?;
        let context = host.configured_local_context_tokens(&profile.model)?;
        write_setup_text(writer, &format!("Local coding context: {context} tokens. Sizes: 8K, 16K, 32K, 64K, 128K, 256K. Configure outside chat with: argentum provider context llama-cpp --size 32k. New profiles have no fixed response-token cap (EOS/context/cancellation); existing explicit limits are retained.\n")).await?;
        return Ok(());
    }
    if host.selected_model_budget().ok().flatten().is_some() {
        return Ok(());
    }
    let profile = host
        .provider_profiles()?
        .into_iter()
        .find(|p| p.selected)
        .ok_or_else(|| setup_error("select a provider first"))?;
    write_setup_text(writer, "Set all five model limits before the first request: total context, separate input limit, maximum output, requested output, uncertainty allowance. These are owner declarations, not verified provider capacity. Existing declarations are retained.\n").await?;
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(
            reader,
            writer,
            "Limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY: ",
        )
        .await?;
        let limits = match argentum_cli::parse_model_budget_limits(&value) {
            Ok(limits) => limits,
            Err(error) => {
                write_setup_text(writer, &format!("{error}\n")).await?;
                continue;
            }
        };
        match host.declare_model_budget(&profile.id, limits) {
            Ok(_) => return Ok(()),
            Err(error) => write_setup_text(writer, &format!("Limits not saved: {error}\n")).await?,
        }
    }
    Err(setup_error("model limits remain incomplete"))
}

async fn prompt_model<R, W>(
    host: &CommandHost,
    state: &SetupState,
    reader: &mut R,
    writer: &mut W,
    allow_local_model_download: bool,
) -> Result<AppCommand, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let provider_id = state
        .snapshot()
        .provider_id
        .clone()
        .ok_or_else(|| setup_error("onboarding has no selected provider"))?;
    let profile = host
        .provider_profiles()?
        .into_iter()
        .find(|profile| profile.id == provider_id)
        .ok_or_else(|| setup_error("selected provider profile is not available"))?;
    if state.snapshot().guidance == Some(GuidanceLevel::Guided) {
        write_setup_text(
            writer,
            "Guided: choose the model already saved for this provider, or enter another bounded model ID.\n",
        )
        .await?;
    }
    if let Ok(catalog) = request_provider_model_catalog(host, &provider_id).await {
        let models = model_picker::filter_models(&catalog.models, "");
        if !models.is_empty() {
            write_setup_text(
                writer,
                &format!(
                    "Available {} models for {}:\n",
                    provider_catalog_source_label(catalog.source),
                    profile.label
                ),
            )
            .await?;
            for (index, model) in models.iter().enumerate() {
                let context = model
                    .context_window_tokens
                    .map(|tokens| format!("; context {tokens}"))
                    .unwrap_or_else(|| "; context unknown".into());
                write_setup_text(
                    writer,
                    &format!(
                        "  {}. {}  {}{}\n",
                        index + 1,
                        model.id,
                        model.label,
                        context
                    ),
                )
                .await?;
            }
            let default_model = if models
                .iter()
                .any(|model| model.id == catalog.selected_model)
            {
                catalog.selected_model
            } else {
                models[0].id.clone()
            };
            for _ in 0..MAX_ONBOARDING_ATTEMPTS {
                let value = read_setup_line(
                    reader,
                    writer,
                    &format!("Model number or ID [{default_model}]: "),
                )
                .await?;
                let value = if value.is_empty() {
                    default_model.clone()
                } else {
                    value
                };
                match model_picker::select_model(&models, &value, false) {
                    Ok(model) => {
                        let mut candidate = profile.clone();
                        candidate.model = model.clone();
                        if let Some(issue) = coding_capability_issue(host, &candidate)? {
                            write_setup_text(
                                writer,
                                &format!(
                                    "That provider/model cannot run the coding workflow: {issue}\nChoose another displayed model or provider. Your setup remains incomplete.\n"
                                ),
                            )
                            .await?;
                            continue;
                        }
                        return Ok(AppCommand::SetOnboardingModel { provider_id, model });
                    }
                    Err(error) => {
                        write_setup_text(
                            writer,
                            &format!("{error}. Choose a displayed number or exact model ID.\n"),
                        )
                        .await?;
                    }
                }
            }
            return Err(setup_error("too many invalid model choices"));
        }
    }
    if profile.kind == argentum_domain::ProviderKind::LocalLlamaCpp {
        let runtime_root = AppPaths::discover()?.data_dir.join("runtimes");
        prompt_local_runtime_install(reader, writer, allow_local_model_download, &runtime_root)
            .await?;
        for offer in local_model_offers() {
            write_setup_text(
                writer,
                &format!(
                    "Optional local model offer: {} ({}, {}, {} bytes).\nSource revision: {}\nArtifact revision: {}\nSHA-256: {}\nDownload URL: {}\n",
                    offer.label,
                    offer.license,
                    offer.filename,
                    offer.size_bytes,
                    offer.source_revision,
                    offer.artifact_revision,
                    offer.sha256,
                    offer.download_url
                ),
            )
            .await?;
            if allow_local_model_download {
                let choice = read_setup_line(
                    reader,
                    writer,
                    "Download this verified local model now? [y/N]: ",
                )
                .await?;
                if matches!(choice.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                    let model_directory = AppPaths::discover()?.data_dir.join("models");
                    write_setup_text(
                        writer,
                        "Downloading the local model; this may take a while.\n",
                    )
                    .await?;
                    writer.flush().await?;
                    match download_local_model(*offer, model_directory).await {
                        Ok(_) => {
                            write_setup_text(
                                writer,
                                "Local model download and integrity check passed.\n",
                            )
                            .await?;
                            let mut candidate = profile.clone();
                            candidate.model = offer.id.to_owned();
                            if let Some(issue) = coding_capability_issue(host, &candidate)? {
                                write_setup_text(
                                    writer,
                                    &format!(
                                        "That provider/model cannot run the coding workflow: {issue}\nChoose a supported provider/model. Setup remains incomplete.\n"
                                    ),
                                )
                                .await?;
                                continue;
                            }
                            return Ok(AppCommand::SetOnboardingModel {
                                provider_id,
                                model: offer.id.to_owned(),
                            });
                        }
                        Err(error) => {
                            write_setup_text(
                                writer,
                                &format!(
                                    "Local model download was not completed: {error}. Enter a model ID manually.\n"
                                ),
                            )
                            .await?;
                        }
                    }
                }
            }
        }
        write_setup_text(
            writer,
            "The runtime and model are optional and are never fetched or started without separate confirmation. You may also add a GGUF file supported by the pinned llama.cpp runtime.\n",
        )
        .await?;
    }
    let default_model = state
        .snapshot()
        .model
        .clone()
        .unwrap_or_else(|| profile.model.clone());
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(reader, writer, &format!("Model [{default_model}]: ")).await?;
        let model = if value.is_empty() {
            default_model.clone()
        } else {
            value
        };
        if !model.trim().is_empty()
            && model.chars().count() <= 256
            && !model.chars().any(char::is_control)
        {
            let mut candidate = profile.clone();
            candidate.model = model.clone();
            if let Some(issue) = coding_capability_issue(host, &candidate)? {
                write_setup_text(
                    writer,
                    &format!(
                        "That provider/model cannot run the coding workflow: {issue}\nChoose another provider/model. Setup remains incomplete.\n"
                    ),
                )
                .await?;
                continue;
            }
            return Ok(AppCommand::SetOnboardingModel { provider_id, model });
        }
        write_setup_text(
            writer,
            "The model must be nonempty, at most 256 characters, and contain no control characters.\n",
        )
        .await?;
    }
    Err(setup_error("too many invalid model choices"))
}

async fn prompt_local_runtime_install<R, W>(
    reader: &mut R,
    writer: &mut W,
    allow_download: bool,
    runtime_root: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    for offer in local_runtime_offers() {
        let bundle_path = local_runtime_bundle_path(runtime_root, offer)?;
        write_setup_text(
            writer,
            &format!(
                "Optional contained runtime: llama.cpp {} {} for {} {} ({} bytes, {}).\nSHA-256: {}\nDownload URL: {}\nInstall path: {}\n",
                offer.version,
                offer.backend,
                offer.platform,
                offer.arch,
                offer.size_bytes,
                offer.license,
                offer.sha256,
                offer.download_url,
                bundle_path.display()
            ),
        )
        .await?;
        if runtime_bundle_is_valid(&bundle_path, offer) {
            write_setup_text(
                writer,
                "The verified runtime bundle is already installed.\n",
            )
            .await?;
            continue;
        }
        if !allow_download {
            write_setup_text(
                writer,
                "The runtime bundle was not downloaded in this mode.\n",
            )
            .await?;
            continue;
        }
        let choice = read_setup_line(
            reader,
            writer,
            "Download this verified runtime bundle now? [y/N]: ",
        )
        .await?;
        if !matches!(choice.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            write_setup_text(writer, "The runtime bundle was not downloaded.\n").await?;
            continue;
        }
        write_setup_text(writer, "Downloading the contained runtime bundle.\n").await?;
        writer.flush().await?;
        match install_local_runtime(*offer, runtime_root).await {
            Ok(_) => {
                write_setup_text(
                    writer,
                    "Runtime download, archive validation, and integrity checks passed.\n",
                )
                .await?;
            }
            Err(error) => {
                write_setup_text(
                    writer,
                    &format!("Runtime download was not completed: {error}.\n"),
                )
                .await?;
            }
        }
    }
    Ok(())
}

async fn maybe_activate_local_runtime<R, W>(
    host: &CommandHost,
    profile_id: Option<&str>,
    reader: &mut R,
    writer: &mut W,
    allow_direct_confirmation: bool,
) -> Result<bool, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let profiles = host.provider_profiles()?;
    let profile = match profile_id {
        Some(profile_id) => profiles
            .into_iter()
            .find(|profile| profile.id == profile_id),
        None => profiles.into_iter().find(|profile| profile.selected),
    };
    let Some(profile) = profile else {
        return Ok(false);
    };
    if profile.kind != argentum_domain::ProviderKind::LocalLlamaCpp {
        return Ok(true);
    }
    if host.local_runtime_active(&profile.model).await {
        return Ok(true);
    }
    if !allow_direct_confirmation || !host.local_runtime_configured() {
        return Ok(false);
    }

    let model = profile.model;
    run_local_runtime_terminal_activation(
        &model,
        reader,
        writer,
        || host.prepare_local_runtime(model.clone()),
        |plan| host.local_handle().approve_local_runtime(plan),
        |prepared, approval| host.activate_local_runtime(prepared, approval),
    )
    .await
}

async fn run_local_runtime_terminal_activation<R, W, P, PFut, PE, I, IE, A, AFut, AE>(
    model: &str,
    reader: &mut R,
    writer: &mut W,
    prepare: P,
    issue_approval: I,
    activate: A,
) -> Result<bool, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
    P: FnOnce() -> PFut,
    PFut: Future<Output = Result<PreparedLocalRuntime, PE>>,
    PE: std::fmt::Display,
    I: FnOnce(&LocalRuntimeLaunchPlan) -> Result<LocalRuntimeLaunchApproval, IE>,
    IE: std::error::Error + 'static,
    A: FnOnce(PreparedLocalRuntime, LocalRuntimeLaunchApproval) -> AFut,
    AFut: Future<Output = Result<std::sync::Arc<LocalRuntimeClient>, AE>>,
    AE: std::error::Error + 'static,
{
    let choice = read_setup_line(
        reader,
        writer,
        &format!("Review a contained local runtime launch plan for {model}? [y/N]: "),
    )
    .await?;
    if !matches!(choice.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        write_setup_text(writer, "The optional local runtime was not started.\n").await?;
        return Ok(false);
    }

    let prepared = match prepare().await {
        Ok(prepared) => prepared,
        Err(error) => {
            write_setup_text(
                writer,
                &format!("The optional local runtime remains stopped: {error}.\n"),
            )
            .await?;
            return Ok(false);
        }
    };
    let Some(approval) =
        confirm_local_runtime_launch(reader, writer, prepared.plan(), issue_approval).await?
    else {
        return Ok(false);
    };
    write_setup_text(writer, "Starting the approved contained local runtime.\n").await?;
    writer.flush().await?;
    let client = activate(prepared, approval).await?;
    let lease = client.lease();
    write_setup_text(
        writer,
        &format!(
            "Local runtime active with process {}, runtime generation {}, authority generation {}, and a verified model lease.\n",
            lease.pid(),
            lease.generation(),
            lease.authority_generation_ordinal()
        ),
    )
    .await?;
    Ok(true)
}

async fn confirm_local_runtime_launch<R, W, F, E>(
    reader: &mut R,
    writer: &mut W,
    plan: &LocalRuntimeLaunchPlan,
    issue_approval: F,
) -> Result<Option<LocalRuntimeLaunchApproval>, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnOnce(&LocalRuntimeLaunchPlan) -> Result<LocalRuntimeLaunchApproval, E>,
    E: std::error::Error + 'static,
{
    write_setup_text(writer, &format_local_runtime_launch_plan(plan)).await?;
    let choice = read_setup_line(
        reader,
        writer,
        "Start this contained local runtime now? [y/N]: ",
    )
    .await?;
    if !matches!(choice.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        write_setup_text(writer, "The optional local runtime was not started.\n").await?;
        return Ok(None);
    }
    Ok(Some(issue_approval(plan)?))
}

fn format_local_runtime_launch_plan(plan: &LocalRuntimeLaunchPlan) -> String {
    let mut text = format!(
        "Contained local runtime launch plan:\n  Runtime: {} {}\n  Bundle SHA-256: {}\n  Launcher: {}\n  Launcher SHA-256: {}\n  Model: {}\n  Model path: {}\n  Model bytes: {}\n  Model maximum bytes: {}\n  Managed active process limit: {}\n  Managed CPU rate percent: {}\n  Managed process memory bytes: {}\n  Managed job memory bytes: {}\n  Managed output hash bytes per stream: {}\n  Model SHA-256: {}\n  Bind: {}:{}\n  API prefix: {}\n  Context tokens: {}\n  GPU layers: {}\n  Environment SHA-256: {}\n  Disabled features: {}\n  Exact process arguments:\n",
        plan.runtime_id,
        plan.runtime_version,
        plan.runtime_bundle_sha256,
        plan.program.display(),
        plan.executable_sha256,
        plan.model,
        plan.model_path.display(),
        plan.model_size_bytes,
        plan.max_model_bytes,
        plan.managed_active_process_limit,
        plan.managed_cpu_rate_percent,
        plan.managed_process_memory_bytes,
        plan.managed_job_memory_bytes,
        plan.managed_output_hash_limit,
        plan.model_sha256,
        plan.host,
        plan.port,
        plan.api_prefix,
        plan.context_tokens,
        plan.gpu_layers,
        plan.environment_sha256.0,
        plan.disabled_features.join(", ")
    );
    for argument in &plan.arguments {
        text.push_str("    ");
        text.push_str(argument);
        text.push('\n');
    }
    text.push_str("  Launch binding SHA-256: ");
    text.push_str(&plan.digest().0);
    text.push('\n');
    text
}

struct ProviderCatalogChoice {
    models: Vec<ProviderModel>,
    selected_model: String,
    source: ProviderCatalogSource,
}

async fn request_provider_model_catalog(
    host: &CommandHost,
    provider_id: &str,
) -> Result<ProviderCatalogChoice, Box<dyn std::error::Error>> {
    let client = host.client();
    let mut events = client.subscribe();
    client
        .dispatch(AppCommand::ListProviderModels {
            provider_id: provider_id.to_owned(),
        })
        .await?;
    while let Ok(event) = events.try_recv() {
        if let argentum_domain::AppEvent::ProviderModelsSnapshot {
            provider_id: snapshot_provider_id,
            models,
            selected_model,
            source,
        } = event
        {
            if snapshot_provider_id == provider_id {
                return Ok(ProviderCatalogChoice {
                    models,
                    selected_model,
                    source,
                });
            }
        }
    }
    Err("provider command completed without a model catalog".into())
}

fn provider_catalog_source_label(source: ProviderCatalogSource) -> &'static str {
    match source {
        ProviderCatalogSource::Live => "live",
        ProviderCatalogSource::Cached => "cached",
    }
}

async fn prompt_workspace<R, W>(
    host: &CommandHost,
    state: &SetupState,
    reader: &mut R,
    writer: &mut W,
) -> Result<AppCommand, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    if state.snapshot().guidance == Some(GuidanceLevel::Guided) {
        write_setup_text(
            writer,
            "Guided: choose an accessible directory. Argentum canonicalizes it before persistence.\n",
        )
        .await?;
    }
    let default_workspace = state
        .snapshot()
        .workspace
        .clone()
        .unwrap_or_else(|| host.workspace_root().to_path_buf());
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(
            reader,
            writer,
            &format!("Workspace path [{}]: ", default_workspace.display()),
        )
        .await?;
        let workspace = if value.is_empty() {
            default_workspace.clone()
        } else {
            PathBuf::from(value)
        };
        if std::fs::canonicalize(&workspace)
            .map(|path| path.is_dir())
            .unwrap_or(false)
        {
            return Ok(AppCommand::SetOnboardingWorkspace { workspace });
        }
        write_setup_text(writer, "That workspace is not an accessible directory.\n").await?;
    }
    Err(setup_error("too many invalid workspace choices"))
}

async fn prompt_permission<R, W>(
    host: &CommandHost,
    state: &SetupState,
    reader: &mut R,
    writer: &mut W,
) -> Result<AppCommand, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let harness = host.harness_snapshot()?;
    let profiles = harness
        .execution_profiles
        .iter()
        .filter(|profile| profile.selectable)
        .collect::<Vec<_>>();
    if profiles.is_empty() {
        return Err(setup_error("no permission profiles are available"));
    }
    if state.snapshot().guidance == Some(GuidanceLevel::Guided) {
        write_setup_text(
            writer,
            "Guided uses workspace-write scope and prompts before every mutation or process. Full Access uses unrestricted scope and no Argentum approval prompts, while operating-system permissions and security invariants still apply.\n",
        )
        .await?;
    }
    for profile in &profiles {
        write_setup_text(writer, &format!("  {}  {}\n", profile.id, profile.label)).await?;
    }
    let default_id = profiles
        .iter()
        .find(|profile| profile.selected)
        .map(|profile| profile.id.clone())
        .unwrap_or_else(|| profiles[0].id.clone());
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(
            reader,
            writer,
            &format!("Permission profile ID [{default_id}]: "),
        )
        .await?;
        let profile_id = if value.is_empty() {
            default_id.clone()
        } else {
            value
        };
        if profiles.iter().any(|profile| profile.id == profile_id) {
            return Ok(AppCommand::SetOnboardingPermission { profile_id });
        }
        write_setup_text(writer, "That permission profile is not selectable.\n").await?;
    }
    Err(setup_error("too many invalid permission choices"))
}

async fn show_review<W>(
    host: &CommandHost,
    state: &SetupState,
    writer: &mut W,
    isolated_database: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>>
where
    W: AsyncWrite + Unpin,
{
    let snapshot = state.snapshot();
    let provider_id = snapshot
        .provider_id
        .as_deref()
        .ok_or_else(|| setup_error("onboarding review has no provider"))?;
    let provider = host
        .provider_profiles()?
        .into_iter()
        .find(|profile| profile.id == provider_id)
        .ok_or_else(|| setup_error("onboarding review provider is unavailable"))?;
    let credential = credential_state_label(snapshot.credential_state);
    let workspace = snapshot.workspace.as_deref().map_or_else(
        || "not selected".to_owned(),
        |path| path.display().to_string(),
    );
    let model = snapshot.model.as_deref().unwrap_or("not selected");
    let mut capability_profile = provider.clone();
    capability_profile.model = model.to_owned();
    let coding_capability = coding_capability_issue(host, &capability_profile)?.map_or_else(
        || "supported".to_owned(),
        |issue| format!("unavailable: {issue}"),
    );
    let permission = snapshot
        .permission_profile_id
        .as_deref()
        .unwrap_or("not selected");
    let harness = host.harness_snapshot()?;
    let enabled_tools = harness
        .capabilities
        .iter()
        .filter(|capability| capability.kind == HarnessCapabilityKind::Tool && capability.enabled)
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>();
    let enabled_tools = if enabled_tools.is_empty() {
        "none".to_owned()
    } else {
        enabled_tools.join(", ")
    };
    let data_dir = match isolated_database.and_then(Path::parent) {
        Some(path) => path.to_path_buf(),
        None => AppPaths::discover()?.data_dir,
    };
    write_setup_text(
        writer,
        &format!(
            "Final review\nworkspace: {workspace}\nprovider: {} ({})\nmodel: {model}\ncoding capability: {coding_capability}\ncredential state: {credential}\npermission profile: {permission}\nenabled tools: {enabled_tools}\ninstalled plugins: none in Phase 3\nconfigured MCP servers: none in Phase 3\ndata directory: {}\nnetwork policy: unchanged\nfull access: disabled\nmigration: not selected\nconnection: {}\ncompletion: pending confirmation\n",
            provider.id,
            provider.label,
            data_dir.display(),
            connection_state_label(snapshot.connection_state)
        ),
    )
    .await?;
    Ok(())
}

fn coding_capability_issue(
    host: &CommandHost,
    profile: &argentum_domain::ProviderProfile,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let resolution = host.provider_capabilities(profile)?;
    Ok((!resolution.supports_coding()).then(|| resolution.detail.to_owned()))
}

async fn prompt_confirmation<R, W>(
    reader: &mut R,
    writer: &mut W,
) -> Result<bool, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    for _ in 0..MAX_ONBOARDING_ATTEMPTS {
        let value = read_setup_line(reader, writer, "Complete setup? [y/N]: ").await?;
        match value.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            _ => {
                write_setup_text(
                    writer,
                    "Answer y to complete setup or n to leave it incomplete.\n",
                )
                .await?
            }
        }
    }
    Err(setup_error("too many invalid review choices"))
}

async fn read_setup_line<R, W>(
    reader: &mut R,
    writer: &mut W,
    prompt: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_setup_text(writer, prompt).await?;
    let mut bytes = Vec::with_capacity(MAX_ONBOARDING_INPUT_BYTES);
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            if bytes.is_empty() {
                return Err(setup_error("onboarding input ended before completion"));
            }
            break;
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let requested = newline.map_or(buffer.len(), |index| index + 1);
        let available = MAX_ONBOARDING_INPUT_BYTES + 1 - bytes.len();
        if requested > available {
            reader.consume(available);
            return Err(setup_error("onboarding input exceeds the size limit"));
        }
        bytes.extend_from_slice(&buffer[..requested]);
        reader.consume(requested);
        if newline.is_some() {
            break;
        }
    }
    let line = String::from_utf8(bytes)
        .map_err(|_| setup_error("onboarding input must be valid UTF-8"))?;
    let line = line.trim_end_matches(['\r', '\n']).trim().to_owned();
    if line == "/cancel" {
        return Err(setup_error(
            "setup cancelled; saved settings retained; run 'argentum chat' to resume",
        ));
    }
    Ok(line)
}

async fn write_setup_text<W>(writer: &mut W, text: &str) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(text.as_bytes()).await?;
    writer.flush().await
}

fn setup_error(message: &str) -> Box<dyn std::error::Error> {
    Box::new(io::Error::new(
        io::ErrorKind::InvalidInput,
        message.to_owned(),
    ))
}

fn credential_state_label(state: OnboardingCredentialState) -> &'static str {
    match state {
        OnboardingCredentialState::NotRequired => "not required",
        OnboardingCredentialState::Missing => "missing",
        OnboardingCredentialState::Configured => "configured",
        OnboardingCredentialState::Unavailable => "unavailable",
    }
}

fn connection_state_label(state: argentum_domain::OnboardingConnectionState) -> &'static str {
    match state {
        argentum_domain::OnboardingConnectionState::NotTested => "not tested",
        argentum_domain::OnboardingConnectionState::Testing => "testing",
        argentum_domain::OnboardingConnectionState::Reachable => "reachable",
        argentum_domain::OnboardingConnectionState::Failed => "failed",
    }
}

async fn run_harness_command(
    options: CommonOptions,
    action: HarnessAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let local_authority_observed = direct_local_user_interaction(
        options.non_interactive,
        json,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    );
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            match action {
                HarnessAction::Status => {}
                HarnessAction::SetBackend { enabled } => {
                    host.client()
                        .dispatch(AppCommand::SetLocalBackendEnabled { enabled })
                        .await?;
                }
                HarnessAction::SetModule { module_id, action } => {
                    host.client()
                        .dispatch(AppCommand::SetHarnessModule { module_id, action })
                        .await?;
                }
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
                    let command = AppCommand::SelectExecutionProfile { profile_id };
                    if local_authority_observed {
                        host.local_handle().dispatch(command).await?;
                    } else {
                        host.client().dispatch(command).await?;
                    }
                }
                HarnessAction::SetCapability {
                    capability_id,
                    enabled,
                } => {
                    let command = AppCommand::SetHarnessCapabilityEnabled {
                        capability_id,
                        enabled,
                    };
                    if local_authority_observed {
                        host.local_handle().dispatch(command).await?;
                    } else {
                        host.client().dispatch(command).await?;
                    }
                }
                HarnessAction::Trajectory { session_id } => {
                    let client = host.client();
                    let mut events = client.subscribe();
                    client
                        .dispatch(AppCommand::LoadTrajectory { session_id })
                        .await?;
                    let mut trajectory = None;
                    while let Ok(event) = events.try_recv() {
                        if let argentum_domain::AppEvent::TrajectorySnapshotLoaded(snapshot) = event
                        {
                            trajectory = Some(snapshot);
                        }
                    }
                    let trajectory = trajectory
                        .ok_or("trajectory command completed without a trajectory snapshot")?;
                    print_trajectory(trajectory, json);
                    return Ok(());
                }
            }
            print_harness(host.harness_snapshot()?, json);
            Ok(())
        })
    })
    .await
}

async fn run_tool_list_command(
    options: CommonOptions,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            let harness = host.harness_snapshot()?;
            let tools = host
                .tool_descriptors()
                .into_iter()
                .map(|descriptor| {
                    let capability = harness
                        .capabilities
                        .iter()
                        .find(|capability| capability.id == descriptor.capability_id);
                    serde_json::json!({
                        "id": descriptor.id,
                        "title": descriptor.title,
                        "summary": descriptor.summary,
                        "capability_id": descriptor.capability_id,
                        "capabilities": descriptor.capabilities,
                        "risk": descriptor.risk,
                        "requires_approval": descriptor.requires_approval,
                        "enabled": capability.is_some_and(|capability| capability.enabled),
                        "availability": capability.map(|capability| capability.availability),
                        "readiness": capability.map(|capability| capability.readiness),
                    })
                })
                .collect::<Vec<_>>();
            if json {
                println!("{}", serde_json::json!({ "tools": tools }));
            } else {
                println!("Developer tools: {}", tools.len());
                for tool in tools {
                    let state = if tool["enabled"] == true {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    println!(
                        "  {} [{}]: {}",
                        tool["id"].as_str().unwrap_or("unknown"),
                        state,
                        tool["summary"].as_str().unwrap_or("")
                    );
                }
            }
            Ok(())
        })
    })
    .await
}

async fn run_permissions_command(
    options: CommonOptions,
    action: PermissionAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let local_authority_observed = direct_local_user_interaction(
        options.non_interactive,
        json,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    );
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            match action {
                PermissionAction::List => print_permissions(host, json),
                PermissionAction::Select { profile_id } => {
                    let command = AppCommand::SelectExecutionProfile { profile_id };
                    if local_authority_observed {
                        host.local_handle().dispatch(command).await?;
                    } else {
                        host.client().dispatch(command).await?;
                    }
                    print_permissions(host, json)
                }
                PermissionAction::Grants => print_grants(host, None, json).await,
                PermissionAction::Revoke { grant_id } => {
                    host.client()
                        .dispatch(AppCommand::RevokeApprovalGrant { grant_id })
                        .await?;
                    print_grants(host, Some(grant_id), json).await
                }
            }
        })
    })
    .await
}

fn print_permissions(host: &CommandHost, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = host.harness_snapshot()?;
    let policy = host.execution_policy()?;
    let platform_authority = host.platform_authority_snapshot();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "selected": {
                    "profile_id": policy.profile_id,
                    "sandbox_scope": policy.sandbox_scope,
                    "approval_mode": policy.approval_mode,
                    "capability_enabled": policy.capability_enabled,
                },
                "profiles": snapshot.execution_profiles,
                "platform_authority": platform_authority,
            })
        );
    } else {
        println!("Permission profile: {}", policy.profile_id);
        println!("Sandbox scope: {:?}", policy.sandbox_scope);
        println!("Approval mode: {:?}", policy.approval_mode);
        if let Some(authority) = platform_authority {
            println!("Authority platform: {}", authority.platform());
            println!("Authority requested tier: {:?}", authority.requested_tier());
            println!(
                "Authority active tier: {}",
                authority
                    .active_tier()
                    .map(|tier| format!("{tier:?}"))
                    .unwrap_or_else(|| "none".into())
            );
            println!("Authority state: {:?}", authority.state());
            println!("Authority generation: {}", authority.generation());
            let observed_identity = authority
                .probe()
                .and_then(|probe| probe.observed_identity())
                .unwrap_or("not observed");
            println!("Authority observed identity: {observed_identity}");
            let capabilities = authority
                .probe()
                .map(|probe| probe.capabilities().join(", "))
                .filter(|capabilities| !capabilities.is_empty())
                .unwrap_or_else(|| "none".into());
            println!("Authority capabilities: {capabilities}");
        } else {
            println!("Authority platform: unavailable");
        }
        println!("Profiles:");
        for profile in snapshot.execution_profiles {
            let state = if profile.selected {
                "selected"
            } else if profile.selectable {
                "available"
            } else {
                "derived"
            };
            println!("  {} [{}]: {}", profile.id, state, profile.detail);
        }
    }
    Ok(())
}

async fn print_grants(
    host: &CommandHost,
    revoked: Option<argentum_domain::GrantId>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = host.client();
    let mut events = client.subscribe();
    client.dispatch(AppCommand::ListApprovalGrants).await?;
    let grants = std::iter::from_fn(|| events.try_recv().ok())
        .find_map(|event| match event {
            argentum_domain::AppEvent::ApprovalGrantsSnapshot { grants, coverage } => {
                Some((grants, coverage))
            }
            _ => None,
        })
        .ok_or("grant command completed without a grant snapshot")?;
    let (grants, coverage) = grants;
    if json {
        println!(
            "{}",
            serde_json::json!({ "revoked_grant_id": revoked, "grants": grants, "coverage": coverage })
        );
    } else {
        if let Some(grant_id) = revoked {
            println!("Revoked grant: {grant_id}");
        }
        if let Some(coverage) = coverage {
            println!(
                "Grant history page: {} of {} records; complete={}",
                coverage.returned, coverage.total_count, coverage.complete
            );
        }
        println!("Approval grants: {}", grants.len());
        for grant in grants {
            let state = if grant.revoked_at.is_some() {
                "revoked"
            } else {
                "active"
            };
            println!(
                "  {} [{}]: {:?} {:?}",
                grant.id, state, grant.capability, grant.scope
            );
        }
    }
    Ok(())
}

async fn run_verification_command(
    options: CommonOptions,
    action: VerificationAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            let client = host.client();
            let mut events = client.subscribe();
            match action {
                VerificationAction::List => {
                    client.dispatch(AppCommand::ListVerificationRunners).await?;
                    let runners = std::iter::from_fn(|| events.try_recv().ok())
                        .find_map(|event| match event {
                            argentum_domain::AppEvent::VerificationRunnersSnapshot { runners } => {
                                Some(runners)
                            }
                            _ => None,
                        })
                        .ok_or("verification command completed without a runner snapshot")?;
                    if json {
                        println!("{}", serde_json::json!({ "runners": runners }));
                    } else {
                        println!("Verification runners: {}", runners.len());
                        for runner in runners {
                            println!("  {}", runner.id());
                        }
                    }
                }
                VerificationAction::Inspect => {
                    client
                        .dispatch(AppCommand::InspectVerificationRunners)
                        .await?;
                    let runners = std::iter::from_fn(|| events.try_recv().ok())
                        .find_map(|event| match event {
                            argentum_domain::AppEvent::VerificationRunnerInspectionSnapshot {
                                runners,
                            } => Some(runners),
                            _ => None,
                        })
                        .ok_or("verification command completed without an inspection snapshot")?;
                    if json {
                        println!("{}", serde_json::json!({ "runners": runners }));
                    } else {
                        println!("Verification runner inspection: {}", runners.len());
                        for runner in runners {
                            println!(
                                "  {} [{}; available={}; approval_required={}] {}",
                                runner.id,
                                runner.origin,
                                runner.available,
                                runner.approval_required,
                                runner.reason.unwrap_or_default()
                            );
                        }
                    }
                }
                VerificationAction::Approve { runner } => {
                    let review = host.project_verification_review(&runner)?;
                    if review.input_scope.project_inputs.is_some() {
                        if json || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                            return Err("local authority is required for scoped project verification review".into());
                        }
                        println!("{}", serde_json::to_string_pretty(&review)?);
                        println!("Review actual source, installed dependencies, generated outputs and all resource bounds. The runner executes current-user code, not an OS filesystem/network sandbox. Identities are endpoint observations, not proof against transient changes.");
                        print!("Type APPROVE to authorize this exact reviewed project runner: ");
                        std::io::Write::flush(&mut io::stdout())?;
                        let mut answer = String::new();
                        io::stdin().read_line(&mut answer)?;
                        if answer.trim_end_matches(&['\r', '\n'][..]) != "APPROVE" {
                            return Err("Project verification approval declined; no grant recorded".into());
                        }
                    }
                    host.local_handle()
                        .dispatch(AppCommand::ApproveProjectVerification { runner, expected_binding: Some(review.binding) })
                        .await?;
                    if json {
                        println!("{}", serde_json::json!({ "approved": true }));
                    } else {
                        println!("Project verification approval recorded.");
                    }
                }
                VerificationAction::Status { run_id } => {
                    client
                        .dispatch(AppCommand::LoadVerificationEvidence { run_id })
                        .await?;
                    let evidence = std::iter::from_fn(|| events.try_recv().ok())
                        .find_map(|event| match event {
                            argentum_domain::AppEvent::VerificationEvidenceSnapshot {
                                evidence,
                            } => Some(evidence),
                            _ => None,
                        })
                        .ok_or("verification command completed without an evidence snapshot")?;
                    let revision = host.current_verification_revision().ok();
                    let entries = evidence
                        .iter()
                        .map(|item| {
                            serde_json::json!({
                                "evidence": item,
                                "fresh": host.verification_evidence_is_current(item).unwrap_or(false),
                            })
                        })
                        .collect::<Vec<_>>();
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({ "current_revision": revision, "evidence": entries })
                        );
                    } else {
                        println!("Current verification revision: {}", revision.as_ref().map(|value| value.0.as_str()).unwrap_or("unavailable"));
                        println!("Verification evidence: {}", evidence.len());
                        for item in evidence {
                            let freshness = if host.verification_evidence_is_current(&item).unwrap_or(false) {
                                "fresh"
                            } else {
                                "stale"
                            };
                            println!("  {} [{:?}, {freshness}]", item.runner.id(), item.state);
                        }
                    }
                }
            }
            Ok(())
        })
    })
    .await
}

async fn run_changes_command(
    options: CommonOptions,
    action: ChangeAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let ChangeAction::Page {
        run_id,
        limit,
        cursor,
    } = &action
    {
        let after = cursor.as_deref().map(serde_json::from_str).transpose()?;
        // Do not discover hosted credentials or runtime bundles for journal reads.
        let mut config = HostConfig::in_memory(options.workspace.clone());
        config.database = match &options.database {
            Some(database) => Some(database.clone()),
            None if options.isolated => None,
            None => Some(AppPaths::discover()?.database),
        };
        let page = CommandHost::inspect_existing_change_page(config, *run_id, *limit, after)?;
        if json {
            let mut value = serde_json::to_value(&page)?;
            value["next_cursor"] = page
                .next_cursor
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null);
            println!("{}", serde_json::json!({"page": value}));
        } else {
            println!(
                "Historical changes for {} ({} items, complete: {}):",
                page.run_id,
                page.items.len(),
                page.complete
            );
            for item in &page.items {
                println!("{}  {}  {}  {}", item.id, item.kind, item.state, item.path);
            }
            if let Some(cursor) = &page.next_cursor {
                println!("Next cursor: {}", serde_json::to_string(cursor)?);
            }
        }

        return Ok(());
    }
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            let command = match action {
                ChangeAction::Page { .. } => {
                    return Err("page command must use read-only inspection".into());
                }
                ChangeAction::List { run_id } => AppCommand::LoadChangeSet { run_id },
                ChangeAction::Diff { file_change_id } => {
                    AppCommand::LoadUnifiedDiff { file_change_id }
                }
                ChangeAction::Restore { file_change_id } => {
                    AppCommand::RequestRestore { file_change_id }
                }
                ChangeAction::Recovery => AppCommand::LoadChangeRecovery,
                ChangeAction::RetryRecovery { operation_id } => {
                    AppCommand::RetryChangeRecovery { operation_id }
                }
                ChangeAction::RestoreRecovery { operation_id } => {
                    AppCommand::RequestRecoveryRestore { operation_id }
                }
            };
            let client = host.client();
            let mut events = client.subscribe();
            if let Err(error) = client.dispatch(command).await {
                if json {
                    write_machine_response(
                        &mut tokio::io::stdout(),
                        &AtomicU64::new(0),
                        Some("changes-1".into()),
                        ServerPayload::host_command_failed(&error),
                    )
                    .await?;
                }
                return Err(error.into());
            }
            let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({ "events": emitted }))?
                );
            } else if emitted.is_empty() {
                println!("No change records were returned.");
            } else {
                for event in emitted {
                    print_human_event(event);
                }
            }
            Ok(())
        })
    })
    .await
}

fn print_trajectory(snapshot: argentum_domain::TrajectorySnapshot, json: bool) {
    if json {
        println!("{}", serde_json::json!({ "trajectory": snapshot }));
        return;
    }

    println!("Session trajectory: {}", snapshot.session_id);
    println!("Durable records: {}", snapshot.entries.len());
    if snapshot.truncated {
        println!("Earlier records are not included in this bounded view.");
    }
    for entry in snapshot.entries {
        println!(
            "  {} [{}/{}] {}",
            entry.occurred_at,
            trajectory_kind_label(entry.kind),
            trajectory_state_label(entry.state),
            entry.title
        );
        if !entry.detail.is_empty() {
            println!("    {}", entry.detail);
        }
    }
}

const fn trajectory_kind_label(kind: argentum_domain::TrajectoryKind) -> &'static str {
    match kind {
        argentum_domain::TrajectoryKind::Task => "task",
        argentum_domain::TrajectoryKind::Plan => "plan",
        argentum_domain::TrajectoryKind::Lifecycle => "lifecycle",
        argentum_domain::TrajectoryKind::Tool => "tool",
        argentum_domain::TrajectoryKind::Approval => "approval",
        argentum_domain::TrajectoryKind::Usage => "usage",
        argentum_domain::TrajectoryKind::Change => "change",
        argentum_domain::TrajectoryKind::Verification => "verification",
        argentum_domain::TrajectoryKind::Error => "error",
    }
}

const fn trajectory_state_label(state: argentum_domain::TrajectoryState) -> &'static str {
    match state {
        argentum_domain::TrajectoryState::Neutral => "neutral",
        argentum_domain::TrajectoryState::Active => "active",
        argentum_domain::TrajectoryState::Attention => "attention",
        argentum_domain::TrajectoryState::Success => "success",
        argentum_domain::TrajectoryState::Error => "error",
        argentum_domain::TrajectoryState::Cancelled => "cancelled",
    }
}

fn print_harness(snapshot: argentum_domain::HarnessSnapshot, json: bool) {
    if json {
        println!("{}", serde_json::json!({ "harness": snapshot }));
        return;
    }

    println!("Harness profile: {}", snapshot.selected_profile_id);
    if let Some(backend) = snapshot.local_backend {
        let state = if backend.changing {
            "changing"
        } else if backend.recovery_pending {
            "cleanup needed"
        } else if backend.enabled {
            "enabled"
        } else {
            "disabled"
        };
        println!(
            "Local backend: {} [{}]: {}",
            backend.id, state, backend.detail
        );
    }
    println!("Modules:");
    for module in snapshot.modules {
        let state = if module.recovery_pending {
            "cleanup needed"
        } else if module.enabled {
            "enabled"
        } else if module.installed {
            "disabled"
        } else {
            "not installed"
        };
        println!("  {} [{}]: {}", module.id, state, module.detail);
    }
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
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            let client = host.client();
            let native_delivery = matches!(action, GoalAction::Deliver { .. });
            match action {
                GoalAction::Status => {
                    let goal = host.goal()?;
                    let supervision = goal
                        .as_ref()
                        .map(|goal| host.local_handle().inspect_goal_supervision(goal.id))
                        .transpose()?;
                    if json {
                        println!("{}", serde_json::json!({"goal":goal,"allowance":goal.as_ref().and_then(|goal| goal.accounting.as_ref().map(|account| account.remaining(goal.token_budget))),"execution":host.inspect_goal_accounting()?,"supervision":supervision}));
                    } else {
                        print_goal(goal, false).await?;
                        if let Some(inspection) = supervision {
                            print_goal_supervision(&inspection);
                        }
                    }
                }
                GoalAction::Limits { goal_id, limits } => { print_goal(Some(match limits { Some(limits) => host.local_handle().configure_goal_limits(goal_id, limits)?, None => host.local_handle().remove_goal_aggregate_limits(goal_id)? }), json).await?; }
                GoalAction::Criteria {
                    goal_id,
                    runners,
                    owner_attestation_required,
                } => {
                    let goal = host.local_handle().configure_goal_acceptance(
                        goal_id,
                        &runners,
                        owner_attestation_required,
                    )?;
                    let supervision = host.local_handle().inspect_goal_supervision(goal_id)?;
                    if json {
                        println!("{}", serde_json::json!({"command_status":"accepted","goal":goal,"supervision":supervision}));
                    } else {
                        print_goal(Some(goal), false).await?;
                        print_goal_supervision(&supervision);
                    }
                }
                GoalAction::Attest { goal_id, summary } => {
                    let goal = host
                        .local_handle()
                        .attest_goal_acceptance(goal_id, &summary)?;
                    let supervision = host.local_handle().inspect_goal_supervision(goal_id)?;
                    if json {
                        println!("{}", serde_json::json!({"command_status":"accepted","goal":goal,"supervision":supervision}));
                    } else {
                        print_goal(Some(goal), false).await?;
                        print_goal_supervision(&supervision);
                    }
                }
                GoalAction::Supervise {
                    goal_id,
                    iteration_limit,
                    elapsed_seconds,
                } | GoalAction::Deliver {
                    goal_id,
                    iteration_limit,
                    elapsed_seconds,
                } => {
                    let request = argentum_domain::goal_supervision::SupervisionRequest {
                            goal_id,
                            iteration_limit,
                            elapsed_seconds,
                            pause_on_session_switch: true,
                        };
                    let invocation = if native_delivery {
                        host.local_handle().deliver_goal(request).await?
                    } else {
                        host.local_handle().supervise_goal(request).await?
                    };
                    let goal = host.goal()?;
                    let supervision = host.local_handle().inspect_goal_supervision(goal_id)?;
                    let task_success = invocation.state
                        == argentum_domain::goal_supervision::GoalInvocationState::Verified;
                    if json {
                        println!("{}", serde_json::json!({"command_status":"accepted_and_settled","task_success":task_success,"invocation":invocation,"goal":goal,"supervision":supervision}));
                    } else {
                        println!("Invocation: {}", invocation.id);
                        println!("Invocation state: {:?}", invocation.state);
                        println!(
                            "Stop reason: {}",
                            invocation.stop_reason.as_deref().unwrap_or("none")
                        );
                        println!("Attempts: {}", invocation.attempts.len());
                        println!("Task success: {}", task_success);
                        print_goal_supervision(&supervision);
                    }
                }
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
                        GoalAction::Status
                        | GoalAction::Limits { .. }
                        | GoalAction::Criteria { .. }
                        | GoalAction::Attest { .. }
                        | GoalAction::Supervise { .. }
                        | GoalAction::Deliver { .. } => unreachable!("handled above"),
                    };
                    client.dispatch(command).await?;
                    print_goal(host.goal()?, json).await?;
                }
            }
            Ok(())
        })
    })
    .await
}

fn print_goal_supervision(
    inspection: &argentum_domain::goal_supervision::GoalSupervisionInspection,
) {
    println!("Completion evidence: {}", inspection.completion_status);
    println!("Completion current: {}", inspection.completion_current);
    println!(
        "Interrupted claim blocks reentry: {}",
        inspection.interrupted_claim_blocks_reentry
    );
    for criterion in &inspection.current_criteria {
        println!(
            "Criterion {} [{}]: {:?}; {}",
            criterion.criterion_id,
            criterion.runner.id(),
            criterion.state,
            criterion.detail
        );
    }
    if let Some(invocation) = &inspection.invocation {
        println!(
            "Latest invocation: {} {:?}; attempts {}; stop {}",
            invocation.id,
            invocation.state,
            invocation.attempts.len(),
            invocation.stop_reason.as_deref().unwrap_or("none")
        );
    }
}

async fn print_goal(
    goal: Option<argentum_domain::Goal>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({ "allowance": goal.as_ref().and_then(|goal| goal.accounting.as_ref().map(|account| account.remaining(goal.token_budget))), "goal": goal })
            )?
        );
    } else if let Some(goal) = goal {
        println!("Goal identity: {}", goal.id);
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
        if let Some(accounting) = &goal.accounting {
            println!(
                "Accounting: version {}; started {}; incomplete history: {}",
                accounting.version, accounting.started_at, accounting.incomplete_history
            );
            println!(
                "Observed tokens: {}; outstanding: {}; unresolved: {}; overrun: {}",
                accounting.counters.observed_tokens,
                accounting.counters.outstanding_tokens,
                accounting.counters.unresolved_tokens,
                accounting.counters.overrun_tokens
            );
            println!(
                "Request attempts: {}; tool admissions: {}; admitted runtime: {} ms",
                accounting.counters.request_attempts,
                accounting.counters.tool_admissions,
                accounting.counters.elapsed_milliseconds
            );
            let allowance = accounting.remaining(goal.token_budget);
            println!("Effective client charge: {}; nonrefundable owner-accepted units: {}; provider usage coverage complete: {}",
                allowance.effective_client_charge_tokens,allowance.owner_accepted_nonrefundable_tokens,allowance.provider_usage_coverage_complete);
            println!(
                "Estimated remaining tokens: {:?}; {}",
                allowance.estimated_tokens, allowance.reason
            );
            println!(
                "Stop reason: {}",
                accounting.stop_reason.as_deref().unwrap_or("none")
            );
        } else {
            println!("Accounting unavailable: legacy usage history is incomplete");
        }
        if let Some(acceptance) = &goal.acceptance {
            println!(
                "Acceptance criteria: version {}; {} declared; owner attestation required: {}",
                acceptance.version,
                acceptance.criteria.len(),
                acceptance.owner_attestation_required
            );
        } else {
            println!("Acceptance criteria: not configured");
        }
        println!("Continuation requires an explicit, finite foreground supervisor invocation. Completion applies only to current declared criteria.");
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
    let config = options.clone().host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            match action {
                ProviderCredentialAction::Set => {
                    let secret = if !options.non_interactive
                        && io::stdin().is_terminal()
                        && io::stdout().is_terminal()
                    {
                        prompt_provider_secret(&provider_id)?
                    } else {
                        read_secret_from_stdin().await?
                    };
                    host.set_provider_credential(&provider_id, secret)?;
                    print_credential_status(&provider_id, true, json);
                }
                ProviderCredentialAction::Clear => {
                    host.clear_provider_credential(&provider_id)?;
                    print_credential_status(&provider_id, false, json);
                }
            }
            Ok(())
        })
    })
    .await
}

async fn read_secret_from_stdin() -> Result<SecretValue, Box<dyn std::error::Error>> {
    if io::stdin().is_terminal() {
        return Err("Refusing visible terminal key input. Use interactive 'provider credential set' or a redirected input stream.".into());
    }
    const MAX_CREDENTIAL_BYTES: usize = 16 * 1024;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    tokio::io::stdin()
        .take((MAX_CREDENTIAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > MAX_CREDENTIAL_BYTES {
        return Err("credential input exceeds the supported limit".into());
    }
    let value = std::str::from_utf8(&bytes)?.trim();
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
    allow_local_runtime: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            if allow_local_runtime {
                let mut reader = BufReader::new(tokio::io::stdin());
                let mut writer = tokio::io::stdout();
                maybe_activate_local_runtime(
                    host,
                    Some(&provider_id),
                    &mut reader,
                    &mut writer,
                    true,
                )
                .await?;
            }
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
                    source,
                } = event
                {
                    catalog = Some((provider_id, models, selected_model, source));
                }
            }
            let (provider_id, models, selected_model, source) =
                catalog.ok_or("provider command completed without a model catalog")?;
            if json {
                let value = serde_json::json!({
                    "provider_id": provider_id,
                    "selected_model": selected_model,
                    "source": provider_catalog_source_label(source),
                    "models": models,
                });
                let mut stdout = tokio::io::stdout();
                stdout.write_all(&serde_json::to_vec(&value)?).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            } else {
                println!("source: {}", provider_catalog_source_label(source));
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
        })
    })
    .await
}

async fn run_provider_profile_command(
    options: CommonOptions,
    command: AppCommand,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = options.host_config()?;
    with_one_shot_host(config, move |host| {
        Box::pin(async move {
            let client = host.client();
            let mut events = client.subscribe();
            client.dispatch(command).await?;
            let mut profiles = None;
            while let Ok(event) = events.try_recv() {
                if let argentum_domain::AppEvent::ProviderProfilesSnapshot { profiles: snapshot } =
                    event
                {
                    profiles = Some(snapshot);
                }
            }
            let profiles =
                profiles.ok_or("provider command completed without a profile snapshot")?;
            print_provider_profiles(&profiles, json).await?;
            if !json {
                match host.selected_provider_coding_issue()? {
                    Some(issue) => println!("Coding setup incomplete: {issue}"),
                    None => println!("Local coding configuration ready. Provider connection and task success have not been tested."),
                }
            }
            Ok(())
        })
    })
    .await
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
        argentum_domain::ProviderKind::LocalLlamaCpp => "llama-cpp",
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

async fn print_session_list(
    snapshot: argentum_domain::SessionListSnapshot,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&serde_json::to_vec(&snapshot)?).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        return Ok(());
    }
    for row in snapshot.rows {
        let selected = if row.selected { "*" } else { " " };
        let pinned = if row.pinned { " pinned" } else { "" };
        let archived = if row.archived { " archived" } else { "" };
        println!(
            "{selected} {}  {}  {}{}{}",
            row.id, row.title, row.last_activity_at, pinned, archived
        );
    }
    if snapshot.has_more {
        if let Some(cursor) = snapshot.next_cursor {
            println!("next_cursor: {cursor}");
        }
    }
    Ok(())
}

async fn print_session_row(
    row: argentum_domain::SessionListRow,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(&serde_json::to_vec(&row)?).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    } else {
        let selected = if row.selected {
            "selected"
        } else {
            "not-selected"
        };
        println!(
            "{}  {}  {}  {} pinned={} archived={}",
            row.id, row.title, row.last_activity_at, selected, row.pinned, row.archived
        );
    }
    Ok(())
}

async fn run_recovery_command(
    database: PathBuf,
    action: RecoveryAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        RecoveryAction::Inspect => {
            let inspection = Store::inspect_recovery(&database)?;
            if json {
                write_recovery_json(&inspection).await?;
            } else {
                println!("Database: {}", database.display());
                println!("Ownership: {:?}", inspection.ownership.outcome);
                println!("Reason: {}", inspection.ownership.reason);
                if let Some(generation) = inspection.ownership.generation {
                    println!("Generation: {generation}");
                }
                println!("Runs: {}", inspection.runs.len());
                for run in inspection.runs {
                    println!(
                        "{} {:?} original={} execution={:?} request={:?} effect={:?} known_effects={} unresolved_effects={} accounting={:?} review_required={}",
                        run.run_id,
                        run.kind,
                        run.original_lifecycle,
                        run.execution_state,
                        run.request_state,
                        run.effect_state,
                        run.known_effects.len(),
                        run.unresolved_effects.len(),
                        run.accounting_state,
                        run.admission_blocked,
                    );
                }
                if inspection.runs_truncated {
                    println!("Run output is truncated; reconcile uses bounded batches.");
                }
                println!(
                    "Legacy ambiguities: {}",
                    inspection.legacy_ambiguities.len()
                );
                for ambiguity in inspection.legacy_ambiguities {
                    println!(
                        "{} lifecycle={} reason={}",
                        ambiguity.run_id, ambiguity.last_lifecycle, ambiguity.reason
                    );
                }
                if inspection.legacy_ambiguities_truncated {
                    println!("Legacy ambiguity output is truncated.");
                }
                println!(
                    "Legacy supervision ambiguities: {}",
                    inspection.legacy_work_ambiguities.len()
                );
                for ambiguity in inspection.legacy_work_ambiguities {
                    println!(
                        "{} kind={} state={} goal={} reason={}",
                        ambiguity.id,
                        ambiguity.kind,
                        ambiguity.state,
                        ambiguity.goal_id,
                        ambiguity.reason
                    );
                }
                if inspection.legacy_work_ambiguities_truncated {
                    println!("Legacy supervision ambiguity output is truncated.");
                }
            }
        }
        RecoveryAction::Reconcile => {
            let settlement = Store::reconcile_recovery(&database)?;
            if json {
                write_recovery_json(&settlement).await?;
            } else {
                println!("Database: {}", database.display());
                println!("Ownership: {:?}", settlement.ownership.outcome);
                println!("Reason: {}", settlement.ownership.reason);
                println!("Mutation performed: {}", settlement.mutation_performed);
                if let Some(revision) = settlement.recovery_revision {
                    println!("Recovery revision: {revision}");
                }
                println!("Recovered runs: {}", settlement.recovered_run_ids.len());
                for run_id in settlement.recovered_run_ids {
                    println!("run {run_id}");
                }
                println!(
                    "Recovered invocations: {}",
                    settlement.recovered_invocation_ids.len()
                );
                for invocation_id in settlement.recovered_invocation_ids {
                    println!("invocation {invocation_id}");
                }
                println!("Incomplete bounded batch: {}", settlement.incomplete);
            }
        }
        RecoveryAction::Review { run_id, revision } => {
            let run = Store::review_recovery(&database, run_id, revision)?;
            if json {
                write_recovery_json(&run).await?;
            } else {
                println!("Database: {}", database.display());
                println!("Run: {}", run.run_id);
                println!("Recovery revision: {revision}");
                println!("Session admission hold cleared: {}", !run.admission_blocked);
                println!("Review acknowledges the record; it does not resolve uncertain effects or usage.");
                println!("Execution: {:?}", run.execution_state);
                println!("Accounting: {:?}", run.accounting_state);
                if run.unresolved_effects.is_empty() {
                    println!("Unresolved effects: none");
                } else {
                    println!("Unresolved effects retained:");
                    for reason in run.unresolved_effects {
                        println!("{reason}");
                    }
                }
            }
        }
    }
    Ok(())
}

async fn write_recovery_json<T: serde::Serialize>(
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(&serde_json::to_vec(value)?).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
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
        argentum_domain::AppEvent::ChangeSetSnapshotLoaded(snapshot) => {
            println!(
                "Run {} changes, revision {}: {} records, +{}, -{}",
                snapshot.run_id,
                snapshot.revision,
                snapshot.files.len(),
                snapshot.additions,
                snapshot.removals
            );
            for change in snapshot.files {
                println!("{:?}: {}", change.kind, change.relative_path);
            }
        }
        argentum_domain::AppEvent::UnifiedDiffLoaded(diff) => {
            if diff.available {
                print!("{}", diff.text);
                if diff.truncated {
                    println!("\nDiff truncated at {} bytes.", diff.byte_count);
                }
            } else {
                println!("Diff unavailable: {}", diff.unavailable_reason);
            }
        }
        argentum_domain::AppEvent::ChangeRecoverySnapshotLoaded(snapshot) => {
            if snapshot.operations.is_empty() {
                println!("No change recovery is required.");
            } else {
                println!(
                    "Recovery required: {} operation(s)",
                    snapshot.operations.len()
                );
                for operation in snapshot.operations {
                    println!(
                        "{} {:?}: {}",
                        operation.operation_id, operation.state, operation.relative_path
                    );
                }
            }
        }
        argentum_domain::AppEvent::FileChangeRecorded(record) => {
            println!("Recorded {:?}: {}", record.kind, record.relative_path);
        }
        argentum_domain::AppEvent::RestoreCompleted(result) => {
            println!("Restore recorded: {}", result.relative_path);
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
    Accounting {
        options: CommonOptions,
        action: AccountingAction,
        json: bool,
    },
    LocalChat {
        options: CommonOptions,
        text_only: bool,
    },
    TerminalDiagnostic {
        output: PathBuf,
    },
    Context {
        options: CommonOptions,
    },
    Instructions {
        options: CommonOptions,
        directory: String,
    },
    Interactive {
        options: CommonOptions,
    },
    Unavailable {
        message: String,
    },
    Setup {
        options: CommonOptions,
        json: bool,
    },
    Serve(CommonOptions),
    Run {
        options: CommonOptions,
        prompt: String,
        json: bool,
        goal_id: Option<argentum_domain::GoalId>,
        skill: Option<String>,
        final_result: bool,
        answer_schema: Option<String>,
        answer_schema_sha256: Option<argentum_domain::Sha256Digest>,
    },
    Result {
        options: CommonOptions,
        run_id: argentum_domain::RunId,
    },
    Diagnostics {
        options: CommonOptions,
        run_id: argentum_domain::RunId,
        json: bool,
    },
    Skills {
        options: CommonOptions,
        action: SkillAction,
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
    ProviderBudget {
        options: CommonOptions,
        profile_id: String,
        limits: [u64; 5],
    },
    ProviderContext {
        options: CommonOptions,
        profile_id: String,
        context_tokens: Option<u32>,
    },
    Compact {
        options: CommonOptions,
        deactivate: bool,
        goal_id: Option<argentum_domain::GoalId>,
    },
    ProviderAdd {
        options: CommonOptions,
        preset_id: String,
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
    ChatGptAuth {
        options: CommonOptions,
        action: String,
        account: Option<String>,
    },
    Workspace {
        action: WorkspaceAction,
        json: bool,
    },
    Sessions {
        options: CommonOptions,
        query: String,
        filter: SessionArchiveFilter,
        limit: u16,
        cursor: Option<String>,
        json: bool,
    },
    SessionSelect {
        options: CommonOptions,
        session_id: argentum_domain::SessionId,
        json: bool,
    },
    SessionOrganize {
        options: CommonOptions,
        action: SessionAction,
        json: bool,
    },
    SessionFork {
        options: CommonOptions,
        source_session_id: argentum_domain::SessionId,
        title: String,
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
    Tools {
        options: CommonOptions,
        json: bool,
    },
    Permissions {
        options: CommonOptions,
        action: PermissionAction,
        json: bool,
    },
    Verification {
        options: CommonOptions,
        action: VerificationAction,
        json: bool,
    },
    Changes {
        options: CommonOptions,
        action: ChangeAction,
        json: bool,
    },
    Recovery {
        database: PathBuf,
        action: RecoveryAction,
        json: bool,
    },
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
enum RecoveryAction {
    Inspect,
    Reconcile,
    Review {
        run_id: argentum_domain::RunId,
        revision: argentum_domain::RunId,
    },
}

#[derive(Debug)]
enum AccountingAction {
    Inspect {
        session_id: argentum_domain::SessionId,
        goal_id: argentum_domain::GoalId,
        request_id: argentum_domain::RunId,
    },
    Resolve {
        resolution_file: PathBuf,
        confirm_reservation: Option<u64>,
    },
}

#[derive(Debug)]
enum GoalAction {
    Status,
    Limits {
        goal_id: argentum_domain::GoalId,
        limits: Option<[u64; 9]>,
    },
    Criteria {
        goal_id: argentum_domain::GoalId,
        runners: Vec<argentum_domain::VerificationRunnerId>,
        owner_attestation_required: bool,
    },
    Attest {
        goal_id: argentum_domain::GoalId,
        summary: String,
    },
    Supervise {
        goal_id: argentum_domain::GoalId,
        iteration_limit: u32,
        elapsed_seconds: u64,
    },
    Deliver {
        goal_id: argentum_domain::GoalId,
        iteration_limit: u32,
        elapsed_seconds: u64,
    },
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

#[derive(Debug, PartialEq, Eq)]
enum ChangeAction {
    List {
        run_id: argentum_domain::RunId,
    },
    Page {
        run_id: argentum_domain::RunId,
        limit: usize,
        cursor: Option<String>,
    },
    Diff {
        file_change_id: argentum_domain::FileChangeId,
    },
    Restore {
        file_change_id: argentum_domain::FileChangeId,
    },
    Recovery,
    RetryRecovery {
        operation_id: argentum_domain::ChangeOperationId,
    },
    RestoreRecovery {
        operation_id: argentum_domain::ChangeOperationId,
    },
}

#[derive(Debug)]
enum HarnessAction {
    SetBackend {
        enabled: bool,
    },
    SetModule {
        module_id: String,
        action: argentum_domain::ModuleAction,
    },
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
    Trajectory {
        session_id: Option<argentum_domain::SessionId>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum PermissionAction {
    List,
    Select { profile_id: String },
    Grants,
    Revoke { grant_id: argentum_domain::GrantId },
}

#[derive(Debug, PartialEq, Eq)]
enum VerificationAction {
    List,
    Inspect,
    Approve {
        runner: argentum_domain::VerificationRunnerId,
    },
    Status {
        run_id: Option<argentum_domain::RunId>,
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

#[derive(Debug)]
enum SkillAction {
    List,
    Inspect { name: String },
}

#[derive(Debug)]
enum SessionAction {
    Inspect {
        session_id: argentum_domain::SessionId,
    },
    Title {
        session_id: argentum_domain::SessionId,
        title: String,
    },
    Pin {
        session_id: argentum_domain::SessionId,
        pinned: bool,
    },
    Archive {
        session_id: argentum_domain::SessionId,
        archived: bool,
    },
}

#[derive(Debug, Clone)]
struct CommonOptions {
    workspace: PathBuf,
    database: Option<PathBuf>,
    endpoint: String,
    model: String,
    no_color: bool,
    non_interactive: bool,
    isolated: bool,
}

impl Default for CommonOptions {
    fn default() -> Self {
        Self {
            workspace: default_workspace(),
            database: None,
            endpoint: argentum_cli::DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: argentum_cli::DEFAULT_MODEL.into(),
            no_color: false,
            non_interactive: false,
            isolated: false,
        }
    }
}

fn default_workspace() -> PathBuf {
    if let Some(workspace) = AppPaths::discover()
        .ok()
        .and_then(|paths| paths.load_workspace().ok().flatten())
    {
        return workspace;
    }
    env::current_dir()
        .map(avoid_staging_artifact_workspace)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn avoid_staging_artifact_workspace(path: PathBuf) -> PathBuf {
    let is_name = |candidate: Option<&std::ffi::OsStr>, expected: &str| {
        candidate
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    };
    let parent = path.parent();
    let staging_parent = parent.and_then(Path::parent);
    if is_name(parent.and_then(Path::file_name), "windows-cli")
        && is_name(staging_parent.and_then(Path::file_name), "artifacts")
    {
        return staging_parent
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or(path);
    }
    path
}

impl CommonOptions {
    fn isolated() -> Self {
        Self {
            workspace: PathBuf::new(),
            database: None,
            endpoint: argentum_cli::DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: argentum_cli::DEFAULT_MODEL.into(),
            no_color: true,
            non_interactive: true,
            isolated: true,
        }
    }

    fn host_config(self) -> Result<HostConfig, Box<dyn std::error::Error>> {
        let mut config = if self.isolated {
            HostConfig::in_memory(self.workspace)
        } else {
            HostConfig::discover(self.workspace)?
        };
        if let Some(database) = self.database {
            config.database = Some(database);
        }
        config.provider_endpoint = self.endpoint;
        config.model = self.model;
        Ok(config)
    }
}

/// Canonical retained cursor upper bound, shared by parsing and page dispatch.
const MAX_CHANGE_PAGE_CURSOR_BYTES: usize = 512;

/// Boolean switches are never accepted as the value of a preceding option.
/// Without this guard a reserved token such as `--json` could silently satisfy a
/// missing value and change the meaning of the command.
const RESERVED_BOOLEAN_FLAGS: &[&str] = &[
    "--json",
    "--isolated",
    "--no-color",
    "--non-interactive",
    "--help",
    "-h",
    "--version",
    "-V",
];

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
        return Ok(Arguments::Interactive {
            options: CommonOptions::default(),
        });
    };
    if command == "--non-interactive" && values.len() == 1 {
        let options = CommonOptions {
            non_interactive: true,
            ..CommonOptions::default()
        };
        return Ok(Arguments::Interactive { options });
    }
    if command == "--jsonl" && values.len() == 1 {
        return Ok(Arguments::Serve(CommonOptions::default()));
    }
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
    if command == "recovery" {
        return parse_recovery_args(&values);
    }
    if command == "accounting" {
        return parse_accounting_args(&values);
    }
    // Never discover normal workspace/configuration/credentials for this fixture.
    if command == "terminal-diagnostic" {
        return if values.len() == 4 && values[1] == "--synthetic" && values[2] == "--output" {
            Ok(Arguments::TerminalDiagnostic {
                output: PathBuf::from(&values[3]),
            })
        } else {
            Err("Usage: argentum terminal-diagnostic --synthetic --output ABSOLUTE_NEW_FILE; synthetic text only, F10 stops, 90 second limit, no providers or credentials".into())
        };
    }
    // Isolation is selected before normal workspace or credential discovery.
    let mut options = if values.iter().any(|value| value == "--isolated") {
        let mut isolated = CommonOptions::isolated();
        if matches!(command, "chat" | "setup" | "local-chat" | "local-text") {
            isolated.non_interactive = false;
        }
        isolated
    } else {
        CommonOptions::default()
    };
    let mut json = false;
    let mut final_result = false;
    let mut prompt = None;
    let mut result_run_id = None;
    let mut skill = None;
    let mut answer_schema = None;
    let mut answer_schema_sha256 = None;
    let mut skill_action = None;
    let mut session_action = None;
    let mut session_id = None;
    let mut session_target = None;
    let mut session_title = None;
    let mut session_query = String::new();
    let mut session_filter = SessionArchiveFilter::Active;
    let mut session_limit: u16 = 20;
    let mut session_cursor = None;
    let mut provider_action = None;
    let mut chatgpt_account = None;
    let mut provider_subaction = None;
    let mut provider_id = None;
    let mut provider_label = None;
    let mut provider_kind = None;
    let mut provider_endpoint = None;
    let mut provider_model = None;
    let mut provider_budget_limits = None;
    let mut local_context_tokens = None;
    let mut deactivate_summary = false;
    let mut select_saved_provider = false;
    let mut goal_action = None;
    let mut goal_limits = None;
    let mut goal_remove_aggregate = false;
    let mut goal_limit_id = None;
    let mut goal_runners = None;
    let mut goal_owner_attestation_required = false;
    let mut goal_attestation_summary = None;
    let mut goal_invocation_iterations = None;
    let mut goal_invocation_seconds = None;
    let mut associated_goal = None;
    let mut goal_objective = None;
    let mut goal_token_budget = None;
    let mut goal_tool_budget = None;
    let mut goal_time_budget_seconds = None;
    let mut workspace_action = None;
    let mut workspace_path = None;
    let mut harness_action = None;
    let mut harness_target = None;
    let mut harness_visibility = None;
    let mut changes_action = None;
    let mut changes_subaction = None;
    let mut changes_target = None;
    let mut changes_limit = None;
    let mut changes_cursor = None;
    let mut tool_action = None;
    let mut instruction_directory = String::new();
    let mut permission_action = None;
    let mut permission_target = None;
    let mut verification_action = None;
    let mut verification_target = None;
    let mut index = 1;
    while index < values.len() {
        match values[index].as_str() {
            "--goal-id"
                if command == "goal"
                    && matches!(
                        goal_action.as_deref(),
                        Some("limits" | "criteria" | "attest" | "supervise" | "deliver")
                    ) =>
            {
                if goal_limit_id.is_some() {
                    return Err("--goal-id may be supplied once".into());
                }
                index += 1;
                goal_limit_id = Some(
                    required_value(&values, index, "--goal-id")?
                        .parse::<argentum_domain::GoalId>()
                        .map_err(|_| "--goal-id requires an exact UUID".to_owned())?,
                );
            }
            "--limits" if command == "goal" && goal_action.as_deref() == Some("limits") => {
                if goal_limits.is_some() || goal_remove_aggregate {
                    return Err("--limits may be supplied once".into());
                }
                index += 1;
                let limits = required_value(&values, index, "--limits")?
                    .split(',')
                    .map(str::parse::<u64>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "--limits requires nine positive integers".to_owned())?;
                goal_limits = Some(<[u64; 9]>::try_from(limits).map_err(|_| "--limits requires GOAL_TOKENS,REQUESTS,ITERATIONS,TOOLS,SECONDS,RUN_TOKENS,RUN_REQUESTS,RUN_TOOLS,RUN_SECONDS".to_owned())?);
            }
            "--remove-aggregate"
                if command == "goal" && goal_action.as_deref() == Some("limits") =>
            {
                if goal_remove_aggregate || goal_limits.is_some() {
                    return Err(
                        "--remove-aggregate is exclusive with --limits and may be supplied once"
                            .into(),
                    );
                }
                goal_remove_aggregate = true;
            }
            "--runners" if command == "goal" && goal_action.as_deref() == Some("criteria") => {
                if goal_runners.is_some() {
                    return Err("--runners may be supplied once".into());
                }
                index += 1;
                goal_runners = Some(
                    required_value(&values, index, "--runners")?
                        .split(',')
                        .map(parse_verification_runner)
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            "--owner-attestation-required"
                if command == "goal" && goal_action.as_deref() == Some("criteria") =>
            {
                goal_owner_attestation_required = true;
            }
            "--summary" if command == "goal" && goal_action.as_deref() == Some("attest") => {
                if goal_attestation_summary.is_some() {
                    return Err("--summary may be supplied once".into());
                }
                index += 1;
                goal_attestation_summary =
                    Some(required_value(&values, index, "--summary")?.to_owned());
            }
            "--iterations"
                if command == "goal"
                    && matches!(goal_action.as_deref(), Some("supervise" | "deliver")) =>
            {
                if goal_invocation_iterations.is_some() {
                    return Err("--iterations may be supplied once".into());
                }
                index += 1;
                goal_invocation_iterations = Some(
                    required_value(&values, index, "--iterations")?
                        .parse::<u32>()
                        .map_err(|_| "--iterations requires a positive integer".to_owned())?,
                );
            }
            "--elapsed-seconds"
                if command == "goal"
                    && matches!(goal_action.as_deref(), Some("supervise" | "deliver")) =>
            {
                if goal_invocation_seconds.is_some() {
                    return Err("--elapsed-seconds may be supplied once".into());
                }
                index += 1;
                goal_invocation_seconds = Some(
                    required_value(&values, index, "--elapsed-seconds")?
                        .parse::<u64>()
                        .map_err(|_| "--elapsed-seconds requires a positive integer".to_owned())?,
                );
            }
            "--deactivate" if command == "compact" => deactivate_summary = true,
            "--title" if command == "session" && session_action.as_deref() == Some("fork") => {
                if session_title.is_some() {
                    return Err("session fork accepts --title only once".into());
                }
                index += 1;
                session_title = Some(required_value(&values, index, "--title")?.to_owned());
            }
            "--size" if command == "provider" && provider_action.as_deref() == Some("context") => {
                if local_context_tokens.is_some() {
                    return Err("provider context accepts --size only once".into());
                }
                index += 1;
                local_context_tokens = Some(
                    argentum_providers::parse_sidecar_context(required_value(
                        &values, index, "--size",
                    )?)
                    .map_err(|e| e.to_string())?,
                );
            }
            "--limits" if command == "provider" && provider_action.as_deref() == Some("budget") => {
                if provider_budget_limits.is_some() {
                    return Err("provider budget accepts --limits only once".into());
                }
                index += 1;
                provider_budget_limits = Some(argentum_cli::parse_model_budget_limits(
                    required_value(&values, index, "--limits")?,
                )?);
            }
            "--directory" if command == "instructions" => {
                index += 1;
                instruction_directory = required_value(&values, index, "--directory")?.to_owned();
            }
            "--query" if command == "sessions" => {
                index += 1;
                session_query = required_value(&values, index, "--query")?.to_owned();
            }
            "--limit" if command == "sessions" => {
                index += 1;
                session_limit = required_value(&values, index, "--limit")?
                    .parse()
                    .map_err(|_| "sessions --limit requires an integer from 1 to 100".to_owned())?;
            }
            "--cursor" if command == "sessions" => {
                index += 1;
                session_cursor = Some(required_value(&values, index, "--cursor")?.to_owned());
            }
            "--archived" if command == "sessions" => {
                session_filter = SessionArchiveFilter::Archived
            }
            "--all" if command == "sessions" => session_filter = SessionArchiveFilter::All,
            "--run-id" if command == "changes" && changes_action.as_deref() == Some("page") => {
                if changes_target.is_some() {
                    return Err("--run-id may be supplied once".into());
                }
                index += 1;
                changes_target = Some(required_value(&values, index, "--run-id")?.to_owned());
            }
            "--limit" if command == "changes" && changes_action.as_deref() == Some("page") => {
                if changes_limit.is_some() {
                    return Err("--limit may be supplied once".into());
                }
                index += 1;
                let n = required_value(&values, index, "--limit")?
                    .parse::<usize>()
                    .map_err(|_| "--limit must be an integer from 1 through 128".to_owned())?;
                if !(1..=128).contains(&n) {
                    return Err("--limit must be from 1 through 128".into());
                }
                changes_limit = Some(n);
            }
            "--cursor" if command == "changes" && changes_action.as_deref() == Some("page") => {
                if changes_cursor.is_some() {
                    return Err("--cursor may be supplied once".into());
                }
                index += 1;
                let cursor = required_value(&values, index, "--cursor")?;
                if cursor.len() > MAX_CHANGE_PAGE_CURSOR_BYTES {
                    return Err(format!(
                        "--cursor exceeds {MAX_CHANGE_PAGE_CURSOR_BYTES} bytes"
                    ));
                }
                changes_cursor = Some(cursor.to_owned());
            }
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
            "--skill" if command == "run" => {
                if skill.is_some() {
                    return Err("--skill may be provided only once".into());
                }
                index += 1;
                skill = Some(required_value(&values, index, "--skill")?.to_owned());
            }
            "--final-result" if command == "run" => {
                if final_result {
                    return Err("--final-result may be supplied only once".into());
                }
                final_result = true;
            }
            "--answer-schema" if command == "run" => {
                if answer_schema.is_some() {
                    return Err("--answer-schema may be supplied only once".into());
                }
                index += 1;
                answer_schema = Some(required_value(&values, index, "--answer-schema")?.to_owned());
            }
            "--answer-schema-sha256" if command == "run" => {
                if answer_schema_sha256.is_some() {
                    return Err("--answer-schema-sha256 may be supplied only once".into());
                }
                index += 1;
                let value = required_value(&values, index, "--answer-schema-sha256")?;
                if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(
                        "--answer-schema-sha256 requires exactly 64 hexadecimal characters".into(),
                    );
                }
                answer_schema_sha256 =
                    Some(argentum_domain::Sha256Digest(value.to_ascii_lowercase()));
            }
            "--goal" if matches!(command, "run" | "compact") => {
                index += 1;
                associated_goal = Some(
                    required_value(&values, index, "--goal")?
                        .parse::<argentum_domain::GoalId>()
                        .map_err(|_| "--goal requires an exact goal UUID".to_owned())?,
                );
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
            "--no-color" => options.no_color = true,
            "--non-interactive" => options.non_interactive = true,
            "--isolated" => {}
            "--jsonl" if command == "serve" => {}
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option '{unknown}'"));
            }
            value if command == "run" && prompt.is_none() => prompt = Some(value.to_owned()),
            value if matches!(command, "result" | "diagnostics") && result_run_id.is_none() => {
                result_run_id = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{command} requires a valid RUN_ID"))?,
                )
            }
            value if command == "skills" && skill_action.is_none() => {
                skill_action = Some(value.to_owned())
            }
            value
                if command == "skills"
                    && skill_action.as_deref() == Some("inspect")
                    && skill.is_none() =>
            {
                skill = Some(value.to_owned())
            }
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
            value
                if command == "session"
                    && matches!(
                        session_action.as_deref(),
                        Some(
                            "inspect"
                                | "title"
                                | "pin"
                                | "unpin"
                                | "archive"
                                | "unarchive"
                                | "fork"
                        )
                    )
                    && session_target.is_none() =>
            {
                session_target = Some(
                    value
                        .parse()
                        .map_err(|_| "session command requires a valid session ID".to_owned())?,
                )
            }
            value
                if command == "session"
                    && session_action.as_deref() == Some("title")
                    && session_target.is_some()
                    && session_title.is_none() =>
            {
                session_title = Some(value.to_owned())
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
            value if command == "changes" && changes_action.is_none() => {
                changes_action = Some(value.to_owned())
            }
            value if command == "tool" && tool_action.is_none() => {
                tool_action = Some(value.to_owned())
            }
            value if command == "permissions" && permission_action.is_none() => {
                permission_action = Some(value.to_owned())
            }
            value
                if command == "permissions"
                    && matches!(permission_action.as_deref(), Some("select" | "revoke"))
                    && permission_target.is_none() =>
            {
                permission_target = Some(value.to_owned())
            }
            value if command == "verification" && verification_action.is_none() => {
                verification_action = Some(value.to_owned())
            }
            value
                if command == "verification"
                    && matches!(verification_action.as_deref(), Some("status" | "approve"))
                    && verification_target.is_none() =>
            {
                verification_target = Some(value.to_owned())
            }
            value
                if command == "changes"
                    && changes_action.as_deref() == Some("recovery")
                    && matches!(value, "retry" | "restore")
                    && changes_subaction.is_none() =>
            {
                changes_subaction = Some(value.to_owned())
            }
            value
                if command == "changes"
                    && matches!(changes_action.as_deref(), Some("list" | "diff" | "restore"))
                    && changes_target.is_none() =>
            {
                changes_target = Some(value.to_owned())
            }
            value
                if command == "changes"
                    && changes_action.as_deref() == Some("recovery")
                    && changes_subaction.is_some()
                    && changes_target.is_none() =>
            {
                changes_target = Some(value.to_owned())
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
                        Some(
                            "profile"
                                | "surface"
                                | "execution"
                                | "capability"
                                | "trajectory"
                                | "module"
                                | "backend"
                        )
                    )
                    && harness_target.is_none() =>
            {
                harness_target = Some(value.to_owned())
            }
            value
                if command == "harness"
                    && matches!(
                        harness_action.as_deref(),
                        Some("surface" | "capability" | "module")
                    )
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
                        Some(
                            "probe"
                                | "add"
                                | "save"
                                | "select"
                                | "models"
                                | "model"
                                | "budget"
                                | "context"
                                | "login"
                                | "login-new"
                                | "logout"
                                | "accounts"
                                | "account"
                        )
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
            value
                if command == "provider"
                    && provider_action.as_deref() == Some("account")
                    && provider_id.as_deref() == Some("chatgpt")
                    && chatgpt_account.is_none() =>
            {
                chatgpt_account = Some(value.to_owned());
            }
            value => return Err(format!("unexpected argument '{value}'")),
        }
        index += 1;
    }
    if options.isolated
        && (!options.workspace.is_absolute()
            || !options
                .database
                .as_ref()
                .is_some_and(|path| path.is_absolute())
            || !matches!(
                command,
                "status"
                    | "chat"
                    | "setup"
                    | "local-chat"
                    | "local-text"
                    | "sessions"
                    | "session"
                    | "provider"
                    | "harness"
                    | "tool"
                    | "instructions"
                    | "context"
                    | "compact"
                    | "verification"
                    | "changes"
                    | "permissions"
                    | "goal"
                    | "run"
                    | "result"
                    | "diagnostics"
                    | "skills"
                    | "serve"
            ))
    {
        return Err("--isolated requires absolute --workspace and --database paths; global workspace changes are unavailable".into());
    }
    match command {
        "instructions" if prompt.is_some() => Err("instructions does not accept a prompt".into()),
        "instructions" => Ok(Arguments::Instructions { options, directory: instruction_directory }),
        "context" if prompt.is_some() => Err("context does not accept a prompt".into()),
        "context" => Ok(Arguments::Context { options }),
        "compact" if prompt.is_some() => Err("compact does not accept a prompt".into()),
        "compact" if values.iter().any(|value| matches!(value.as_str(), "--model" | "--endpoint")) => Err("compact uses the saved selected profile; configure/select a profile separately".into()),
        "compact" if deactivate_summary && associated_goal.is_some() => Err("--deactivate is provider-free; omit --goal".into()),
        "compact" => Ok(Arguments::Compact { options, deactivate: deactivate_summary, goal_id: associated_goal }),
        "chat" if prompt.is_some() => Err("chat does not accept a prompt; use 'run'".into()),
        "chat" => Ok(Arguments::Interactive { options }),
        "local-chat" | "local-text" if prompt.is_none() => Ok(Arguments::LocalChat { options, text_only: command == "local-text" }),
        "doctor" | "model" | "profile" | "module" | "plugin" | "mcp" | "migrate"
        | "prompt" | "self" | "update" => {
            Ok(Arguments::Unavailable {
                message: format!("{command} is not available in this Genesis phase"),
            })
        }
        "setup" if prompt.is_some() => Err("setup does not accept a prompt".into()),
        "setup" => Ok(Arguments::Setup { options, json }),
        "serve" if json => Err("serve always uses JSONL; remove --json".into()),
        "serve" if prompt.is_some() => Err("serve does not accept a prompt".into()),
        "serve" => Ok(Arguments::Serve(options)),
        "run" if final_result && json => {
            Err("--final-result cannot be combined with --json; it emits one result envelope".into())
        }
        "run" if final_result && (associated_goal.is_some() || skill.is_some()) => {
            Err("--final-result supports ordinary tasks only".into())
        }
        "run" if answer_schema.is_some() && !final_result => {
            Err("--answer-schema requires --final-result".into())
        }
        "run" if answer_schema_sha256.is_some() && answer_schema.is_none() => {
            Err("--answer-schema-sha256 requires --answer-schema".into())
        }
        "run" => Ok(Arguments::Run {
            options,
            prompt: prompt.ok_or_else(|| "run requires a prompt".to_owned())?,
            json,
            goal_id: associated_goal,
            skill,
            final_result,
            answer_schema,
            answer_schema_sha256,
        }),
        "result" if prompt.is_some() => Err("result does not accept a prompt".into()),
        "result" if json => Err("result emits one JSON result envelope; remove --json".into()),
        "result" => Ok(Arguments::Result {
            options,
            run_id: result_run_id.ok_or_else(|| "result requires RUN_ID".to_owned())?,
        }),
        "diagnostics" if prompt.is_some() => Err("diagnostics does not accept a prompt".into()),
        "diagnostics" => Ok(Arguments::Diagnostics {
            options,
            run_id: result_run_id.ok_or_else(|| "diagnostics requires RUN_ID".to_owned())?,
            json,
        }),
        "skills" if prompt.is_some() => Err("skills does not accept a prompt".into()),
        "skills" => match skill_action.as_deref() {
            Some("list") if skill.is_none() => Ok(Arguments::Skills {
                options,
                action: SkillAction::List,
                json,
            }),
            Some("inspect") => Ok(Arguments::Skills {
                options,
                action: SkillAction::Inspect {
                    name: skill.ok_or_else(|| "skills inspect requires NAME".to_owned())?,
                },
                json,
            }),
            _ => Err("skills requires 'list' or 'inspect NAME'".into()),
        },
        "status" if prompt.is_some() => Err("status does not accept a prompt".into()),
        "status" => Ok(Arguments::Status { options, json }),
        "tool" if prompt.is_some() => Err("tool does not accept a prompt".into()),
        "tool" if tool_action.as_deref() == Some("list") => {
            Ok(Arguments::Tools { options, json })
        }
        "tool" => Err("tool requires 'list'".into()),
        "permissions" if prompt.is_some() => {
            Err("permissions does not accept a prompt".into())
        }
        "permissions" => {
            let action = match permission_action.as_deref() {
                Some("list") if permission_target.is_none() => PermissionAction::List,
                Some("select") => PermissionAction::Select {
                    profile_id: permission_target
                        .ok_or_else(|| "permissions select requires PROFILE_ID".to_owned())?,
                },
                Some("grants") if permission_target.is_none() => PermissionAction::Grants,
                Some("revoke") => PermissionAction::Revoke {
                    grant_id: permission_target
                        .ok_or_else(|| "permissions revoke requires GRANT_ID".to_owned())?
                        .parse()
                        .map_err(|_| "permissions revoke requires a valid GRANT_ID".to_owned())?,
                },
                _ => {
                    return Err(
                        "permissions requires 'list', 'select PROFILE_ID', 'grants', or 'revoke GRANT_ID'"
                            .into(),
                    )
                }
            };
            Ok(Arguments::Permissions {
                options,
                action,
                json,
            })
        }
        "verification" if prompt.is_some() => {
            Err("verification does not accept a prompt".into())
        }
        "verification" => {
            let action = match verification_action.as_deref() {
                Some("list") if verification_target.is_none() => VerificationAction::List,
                Some("inspect") if verification_target.is_none() => VerificationAction::Inspect,
                Some("approve") => VerificationAction::Approve {
                    runner: parse_verification_runner(
                        verification_target
                            .ok_or_else(|| "verification approve requires RUNNER".to_owned())?
                            .as_str(),
                    )?,
                },
                Some("status") => VerificationAction::Status {
                    run_id: verification_target
                        .map(|value| {
                            value.parse().map_err(|_| {
                                "verification status requires a valid RUN_ID".to_owned()
                            })
                        })
                        .transpose()?,
                },
                _ => return Err("verification requires 'list', 'inspect', 'approve RUNNER', or 'status [RUN_ID]'".into()),
            };
            Ok(Arguments::Verification {
                options,
                action,
                json,
            })
        }
        "provider" if prompt.is_some() => Err("provider does not accept a prompt".into()),
        "provider" => match provider_action.as_deref() {
            Some(action @ ("login" | "login-new" | "logout" | "accounts" | "account")) => {
                if provider_id.as_deref()!=Some("chatgpt") || json || values.iter().any(|value|matches!(value.as_str(),"--endpoint"|"--model"|"--label"|"--kind"|"--select"|"--limits")) || provider_model.is_some() || provider_endpoint.is_some() || provider_kind.is_some() || select_saved_provider {
                    return Err("ChatGPT account commands require chatgpt; no credential or endpoint arguments are accepted".into());
                }
                if action=="account" && chatgpt_account.is_none(){return Err("provider account chatgpt requires a saved account ID from provider accounts chatgpt".into());}
                Ok(Arguments::ChatGptAuth{options,action:action.into(),account:chatgpt_account})
            },
            Some("context") => {
                if provider_id.as_deref()!=Some("llama-cpp") || values.iter().any(|v|matches!(v.as_str(),"--model"|"--endpoint"|"--label"|"--kind"|"--select")) {
                    return Err("provider context requires llama-cpp and changes only its context setting".into());
                }
                Ok(Arguments::ProviderContext{options,profile_id:provider_id.unwrap(),context_tokens:local_context_tokens})
            }
            Some("budget") if values.iter().any(|value| matches!(value.as_str(), "--model" | "--endpoint" | "--label" | "--kind" | "--select")) => Err("provider budget changes limits only; save or select the profile separately".into()),
            Some("budget") => Ok(Arguments::ProviderBudget {
                options,
                profile_id: provider_id.ok_or("provider budget requires PROFILE_ID")?,
                limits: provider_budget_limits.ok_or("provider budget requires --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY")?,
            }),
            Some("probe") => Ok(Arguments::ProviderProbe {
                options,
                provider_id: provider_id
                    .unwrap_or_else(|| argentum_cli::DEFAULT_PROVIDER_ID.to_owned()),
                json,
            }),
            Some("list") if provider_id.is_none() => Ok(Arguments::ProviderList { options, json }),
            Some("add") => {
                let preset_id = provider_id.ok_or("provider add requires minimax, deepseek, openai, or chatgpt")?;
                if !matches!(preset_id.as_str(), "minimax" | "deepseek" | "openai" | "chatgpt")
                    || values.iter().any(|value| matches!(value.as_str(), "--endpoint" | "--model"))
                    || provider_label.is_some() || provider_kind.is_some()
                    || provider_endpoint.is_some() || provider_model.is_some() || select_saved_provider {
                    return Err("provider add accepts only minimax, deepseek, openai, or chatgpt; use provider save for custom configuration".into());
                }
                Ok(Arguments::ProviderAdd { options, preset_id, json })
            }
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
                "provider requires 'list', 'add', 'save', 'select', 'models', 'model', 'budget', 'context', 'credential', 'probe', 'login', 'login-new', 'logout', 'accounts', or 'account'".into(),
            ),
        },
        "sessions" if prompt.is_some() => Err("sessions does not accept a prompt".into()),
        "sessions" => Ok(Arguments::Sessions {
            options,
            query: session_query,
            filter: session_filter,
            limit: session_limit,
            cursor: session_cursor,
            json,
        }),
        "session" if session_action.as_deref() != Some("select") => {
            let target = session_target
                .ok_or_else(|| "session requires a session action and SESSION_ID".to_owned())?;
            if session_action.as_deref() == Some("fork") {
                return Ok(Arguments::SessionFork {
                    options,
                    source_session_id: target,
                    title: session_title.ok_or_else(|| {
                        "session fork requires SOURCE_SESSION_ID --title TITLE".to_owned()
                    })?,
                    json,
                });
            }
            let action = match session_action.as_deref() {
                Some("inspect") if session_title.is_none() => SessionAction::Inspect {
                    session_id: target,
                },
                Some("title") => SessionAction::Title {
                    session_id: target,
                    title: session_title.ok_or_else(|| {
                        "session title requires SESSION_ID and TITLE".to_owned()
                    })?,
                },
                Some("pin") if session_title.is_none() => SessionAction::Pin {
                    session_id: target,
                    pinned: true,
                },
                Some("unpin") if session_title.is_none() => SessionAction::Pin {
                    session_id: target,
                    pinned: false,
                },
                Some("archive") if session_title.is_none() => SessionAction::Archive {
                    session_id: target,
                    archived: true,
                },
                Some("unarchive") if session_title.is_none() => SessionAction::Archive {
                    session_id: target,
                    archived: false,
                },
                _ => return Err("session requires 'select ID', 'inspect ID', 'title ID TITLE', 'pin ID', 'unpin ID', 'archive ID', 'unarchive ID', or 'fork ID --title TITLE'".into()),
            };
            Ok(Arguments::SessionOrganize { options, action, json })
        }
        "session" => Ok(Arguments::SessionSelect {
            options,
            session_id: session_id
                .ok_or_else(|| "session select requires a session ID".to_owned())?,
            json,
        }),
        "goal" if prompt.is_some() => Err("goal does not accept a prompt".into()),
        "goal" => match goal_action.as_deref() {
            Some("limits") if goal_objective.is_none() && goal_token_budget.is_none() && goal_tool_budget.is_none() && goal_time_budget_seconds.is_none() && goal_runners.is_none() && goal_attestation_summary.is_none() && goal_invocation_iterations.is_none() && goal_invocation_seconds.is_none() && !goal_owner_attestation_required => {
                if goal_limits.is_none() && !goal_remove_aggregate { return Err("goal limits requires --limits or --remove-aggregate".into()); }
                Ok(Arguments::Goal { options, action: GoalAction::Limits { goal_id: goal_limit_id.ok_or_else(|| "goal limits requires --goal-id".to_owned())?, limits: goal_limits }, json })
            },
            Some("criteria")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none()
                    && goal_limits.is_none()
                    && goal_attestation_summary.is_none()
                    && goal_invocation_iterations.is_none()
                    && goal_invocation_seconds.is_none() =>
            {
                Ok(Arguments::Goal {
                    options,
                    action: GoalAction::Criteria {
                        goal_id: goal_limit_id
                            .ok_or_else(|| "goal criteria requires --goal-id".to_owned())?,
                        runners: goal_runners
                            .ok_or_else(|| "goal criteria requires --runners".to_owned())?,
                        owner_attestation_required: goal_owner_attestation_required,
                    },
                    json,
                })
            }
            Some("attest")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none()
                    && goal_limits.is_none()
                    && goal_runners.is_none()
                    && goal_invocation_iterations.is_none()
                    && goal_invocation_seconds.is_none()
                    && !goal_owner_attestation_required =>
            {
                Ok(Arguments::Goal {
                    options,
                    action: GoalAction::Attest {
                        goal_id: goal_limit_id
                            .ok_or_else(|| "goal attest requires --goal-id".to_owned())?,
                        summary: goal_attestation_summary
                            .ok_or_else(|| "goal attest requires --summary".to_owned())?,
                    },
                    json,
                })
            }
            Some(mode @ ("supervise" | "deliver"))
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none()
                    && goal_limits.is_none()
                    && goal_runners.is_none()
                    && goal_attestation_summary.is_none()
                    && !goal_owner_attestation_required =>
            {
                let goal_id = goal_limit_id.ok_or_else(|| format!("goal {mode} requires --goal-id"))?;
                let iteration_limit = goal_invocation_iterations.ok_or_else(|| format!("goal {mode} requires --iterations"))?;
                let elapsed_seconds = goal_invocation_seconds.ok_or_else(|| format!("goal {mode} requires --elapsed-seconds"))?;
                Ok(Arguments::Goal {
                    options,
                    action: if mode == "deliver" { GoalAction::Deliver { goal_id, iteration_limit, elapsed_seconds } } else { GoalAction::Supervise { goal_id, iteration_limit, elapsed_seconds } },
                    json,
                })
            }
            Some("status")
                if goal_objective.is_none()
                    && goal_token_budget.is_none()
                    && goal_tool_budget.is_none()
                    && goal_time_budget_seconds.is_none()
                    && goal_limits.is_none()
                    && goal_limit_id.is_none()
                    && goal_runners.is_none()
                    && goal_attestation_summary.is_none()
                    && goal_invocation_iterations.is_none()
                    && goal_invocation_seconds.is_none()
                    && !goal_owner_attestation_required =>
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
                Err("goal requires 'status', 'set OBJECTIVE', 'limits', 'criteria', 'attest', 'supervise', 'deliver', 'pause', 'resume', or 'clear'".into())
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
            Some("backend") if harness_visibility.is_none() => {
                let enabled = match harness_target.as_deref() { Some("enable") => true, Some("disable") => false, _ => return Err("harness backend requires enable|disable".into()) };
                Ok(Arguments::Harness { options, action: HarnessAction::SetBackend { enabled }, json })
            }
            Some("module") => {
                let module_id = harness_target.ok_or("harness module requires MODULE_ID")?;
                let action = match harness_visibility.as_deref() {
                    Some("install") => argentum_domain::ModuleAction::Install,
                    Some("enable") => argentum_domain::ModuleAction::Enable,
                    Some("disable") => argentum_domain::ModuleAction::Disable,
                    Some("remove") => argentum_domain::ModuleAction::Remove,
                    _ => return Err("harness module requires install|enable|disable|remove".into()),
                };
                Ok(Arguments::Harness { options, action: HarnessAction::SetModule { module_id, action }, json })
            }
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
            Some("trajectory") if harness_visibility.is_none() => {
                let session_id = harness_target
                    .map(|value| {
                        value
                            .parse()
                            .map_err(|_| "harness trajectory requires a valid SESSION_ID".to_owned())
                    })
                    .transpose()?;
                Ok(Arguments::Harness {
                    options,
                    action: HarnessAction::Trajectory { session_id },
                    json,
                })
            }
            _ => Err("harness requires 'status', 'profile PROFILE_ID', 'surface SURFACE show|hide', 'execution PROFILE_ID', 'capability CAPABILITY_ID enable|disable', 'backend enable|disable', 'module MODULE_ID install|enable|disable|remove', or 'trajectory [SESSION_ID]'".into()),
        },
        "changes" if prompt.is_some() => Err("changes does not accept a prompt".into()),
        "changes" => {
            let parse_target = |label: &str| {
                changes_target
                    .as_deref()
                    .ok_or_else(|| format!("changes {label} requires an identifier"))?
                    .parse()
                    .map_err(|_| format!("changes {label} requires a valid identifier"))
            };
            let action = match (changes_action.as_deref(), changes_subaction.as_deref()) {
                (Some("list"), None) => ChangeAction::List {
                    run_id: parse_target("list")?,
                },
                (Some("page"), None) => {
                    let run_id = changes_target
                        .as_deref()
                        .ok_or("changes page requires --run-id")?
                        .parse()
                        .map_err(|_| "--run-id requires a valid UUID")?;
                    if let Some(text) = changes_cursor.as_deref() {
                        if text.len() > MAX_CHANGE_PAGE_CURSOR_BYTES {
                            return Err(format!(
                                "--cursor exceeds {MAX_CHANGE_PAGE_CURSOR_BYTES} bytes"
                            ));
                        }
                        serde_json::from_str::<argentum_domain::ChangePageCursor>(text)
                            .map_err(|_| "--cursor must be a valid page cursor")?;
                    }
                    ChangeAction::Page {
                        run_id,
                        limit: changes_limit.unwrap_or(64),
                        cursor: changes_cursor,
                    }
                }
                (Some("diff"), None) => ChangeAction::Diff {
                    file_change_id: parse_target("diff")?,
                },
                (Some("restore"), None) => ChangeAction::Restore {
                    file_change_id: parse_target("restore")?,
                },
                (Some("recovery"), None) if changes_target.is_none() => ChangeAction::Recovery,
                (Some("recovery"), Some("retry")) => ChangeAction::RetryRecovery {
                    operation_id: parse_target("recovery retry")?,
                },
                (Some("recovery"), Some("restore")) => ChangeAction::RestoreRecovery {
                    operation_id: parse_target("recovery restore")?,
                },
                _ => return Err("changes requires 'page --run-id RUN_ID', 'list RUN_ID', 'diff FILE_CHANGE_ID', 'restore FILE_CHANGE_ID', 'recovery', 'recovery retry OPERATION_ID', or 'recovery restore OPERATION_ID'".into()),
            };
            Ok(Arguments::Changes { options, action, json })
        }
        unknown => Err(format!("unknown command '{unknown}'")),
    }
}

#[cfg(test)]
mod changes_page_cli_tests {
    use super::*;

    #[test]
    fn changes_page_parser_enforces_bounds_and_accepts_json_contract() {
        let run = "01234567-89ab-cdef-0123-456789abcdef";
        let default_page = parse_args(["changes", "page", "--run-id", run, "--json"]).unwrap();
        assert!(matches!(
            default_page,
            Arguments::Changes {
                action: ChangeAction::Page {
                    limit: 64,
                    cursor: None,
                    ..
                },
                json: true,
                ..
            }
        ));
        let minimum_page =
            parse_args(["changes", "page", "--run-id", run, "--limit", "1", "--json"]).unwrap();
        assert!(matches!(
            minimum_page,
            Arguments::Changes {
                action: ChangeAction::Page { limit: 1, .. },
                ..
            }
        ));
        let cursor = argentum_domain::ChangePageCursor {
            project_id: argentum_domain::ProjectId::new_v4(),
            session_id: argentum_domain::SessionId::new_v4(),
            run_id: run.parse().unwrap(),
            stamp: 1,
            sequence: 2,
            id: argentum_domain::FileChangeId::new_v4(),
        };
        let cursor_text = serde_json::to_string(&cursor).unwrap();
        let parsed = parse_args([
            "changes",
            "page",
            "--run-id",
            run,
            "--limit",
            "128",
            "--cursor",
            &cursor_text,
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            parsed,
            Arguments::Changes {
                action: ChangeAction::Page { limit: 128, .. },
                json: true,
                ..
            }
        ));
        for field in ["project_id", "session_id", "run_id"] {
            let mut missing = serde_json::to_value(&cursor).unwrap();
            missing.as_object_mut().unwrap().remove(field);
            let text = serde_json::to_string(&missing).unwrap();
            assert!(
                parse_args(["changes", "page", "--run-id", run, "--cursor", &text, "--json"])
                    .is_err()
            );
        }
        assert!(parse_args(["changes", "page", "--run-id", "not-a-uuid", "--json"]).is_err());
        assert!(parse_args([
            "changes",
            "page",
            "--run-id",
            run,
            "--unknown",
            "value",
            "--json"
        ])
        .is_err());
        for limit in ["0", "129", "999999999999999999999999999"] {
            assert!(
                parse_args(["changes", "page", "--run-id", run, "--limit", limit, "--json"])
                    .is_err()
            );
        }
        assert!(
            parse_args(["changes", "page", "--run-id", run, "--cursor", "not-json", "--json"])
                .is_err()
        );
        assert!(parse_args([
            "changes",
            "page",
            "--run-id",
            run,
            "--cursor",
            &"x".repeat(257),
            "--json"
        ])
        .is_err());
        assert!(parse_args([
            "changes", "page", "--run-id", run, "--limit", "64", "--limit", "1", "--json"
        ])
        .is_err());

        assert!(parse_args([
            "changes",
            "page",
            "--run-id",
            run,
            "--unknown",
            "value",
            "--json"
        ])
        .is_err());
    }
}

#[cfg(test)]
mod context_variant_tests {
    use super::*;
    #[test]
    fn context_variants_cli_parser_accepts_presets_and_rejects_ambiguous_configuration() {
        for size in ["8k", "16k", "32K", "64k", "128k", "256k", "32768"] {
            let args = parse_args([
                "provider",
                "context",
                "llama-cpp",
                "--size",
                size,
                "--isolated",
                "--workspace",
                r"A:\fixture",
                "--database",
                r"A:\fixture\state.db",
            ])
            .unwrap();
            assert!(matches!(
                args,
                Arguments::ProviderContext {
                    context_tokens: Some(_),
                    ..
                }
            ));
        }
        for invalid in ["0", "4k", "12k", "512k", "-1", "0x8000"] {
            assert!(parse_args([
                "provider",
                "context",
                "llama-cpp",
                "--size",
                invalid,
                "--isolated"
            ])
            .is_err());
        }
        assert!(parse_args([
            "provider",
            "context",
            "llama-cpp",
            "--size",
            "32k",
            "--size",
            "64k",
            "--isolated"
        ])
        .is_err());
        assert!(parse_args([
            "provider",
            "context",
            "minimax",
            "--size",
            "32k",
            "--isolated"
        ])
        .is_err());
        assert!(parse_args([
            "provider",
            "context",
            "llama-cpp",
            "--size",
            "32k",
            "--model",
            "other.gguf",
            "--isolated"
        ])
        .is_err());
    }
}

#[cfg(test)]
mod genesis_polish_parser_tests {
    use super::*;

    fn error_of(values: &[&str]) -> String {
        match parse_args(values.iter().copied()) {
            Ok(arguments) => panic!("expected a parser error, got {arguments:?}"),
            Err(message) => message,
        }
    }

    #[test]
    fn help_text_lists_implemented_subcommands_and_reserved_commands() {
        let text = help_text();
        for expected in [
            "session inspect|pin|unpin|archive|unarchive SESSION_ID",
            "session title SESSION_ID TITLE",
            "verification inspect",
            "verification approve RUNNER",
            "instructions [--directory RELATIVE_DIRECTORY]",
            "--remove-aggregate",
            "lm-studio",
            "doctor      Diagnostics, planned for a later phase",
        ] {
            assert!(text.contains(expected), "help is missing {expected:?}");
        }
        assert!(text.ends_with('\n'));
        #[cfg(feature = "extensions")]
        assert!(text.contains("argentum extensions help"));
        #[cfg(feature = "artifacts")]
        assert!(text.contains("argentum artifacts help"));
    }

    #[test]
    fn fallback_error_messages_list_every_supported_subcommand() {
        let provider = error_of(&["provider"]);
        for expected in [
            "list",
            "add",
            "save",
            "select",
            "models",
            "model",
            "budget",
            "context",
            "credential",
            "probe",
            "login",
            "login-new",
            "logout",
            "accounts",
            "account",
        ] {
            assert!(
                provider.contains(expected),
                "provider error omits {expected}: {provider}"
            );
        }
        assert!(error_of(&["goal"]).contains("deliver"));
        let harness = error_of(&["harness"]);
        assert!(harness.contains("backend"));
        assert!(harness.contains("module"));
        assert!(error_of(&["changes"]).contains("page"));
        assert!(error_of(&["provider", "add"]).contains("chatgpt"));
        assert!(error_of(&["provider", "add", "synthetic-preset"]).contains("chatgpt"));
        let kind = parse_provider_kind("synthetic-kind").unwrap_err();
        assert!(kind.contains("lm-studio"), "{kind}");
    }

    #[test]
    fn change_page_cursor_bound_accepts_extended_canonical_cursor_and_rejects_overflow() {
        let run = "01234567-89ab-cdef-0123-456789abcdef";
        let cursor = argentum_domain::ChangePageCursor {
            project_id: argentum_domain::ProjectId::new_v4(),
            session_id: argentum_domain::SessionId::new_v4(),
            run_id: run.parse().unwrap(),
            stamp: i64::MIN,
            sequence: i64::MIN,
            id: argentum_domain::FileChangeId::new_v4(),
        };
        let text = serde_json::to_string(&cursor).unwrap();
        assert!(
            text.len() > 256,
            "fixture must exceed the retired 256-byte bound: {}",
            text.len()
        );
        assert!(text.len() <= MAX_CHANGE_PAGE_CURSOR_BYTES);
        assert!(matches!(
            parse_args(["changes", "page", "--run-id", run, "--cursor", &text, "--json"]).unwrap(),
            Arguments::Changes {
                action: ChangeAction::Page {
                    cursor: Some(_),
                    ..
                },
                ..
            }
        ));
        let at_bound = "x".repeat(MAX_CHANGE_PAGE_CURSOR_BYTES);
        assert!(matches!(
            parse_args([
                "changes",
                "page",
                "--run-id",
                run,
                "--cursor",
                &at_bound,
                "--json"
            ]),
            Err(message) if message.contains("valid page cursor")
        ));
        let overflow = "x".repeat(MAX_CHANGE_PAGE_CURSOR_BYTES + 1);
        assert!(matches!(
            parse_args([
                "changes",
                "page",
                "--run-id",
                run,
                "--cursor",
                &overflow,
                "--json"
            ]),
            Err(message) if message.contains("512 bytes")
        ));
    }

    #[test]
    fn reserved_boolean_flags_cannot_satisfy_a_missing_option_value() {
        let run = "01234567-89ab-cdef-0123-456789abcdef";
        for flag in RESERVED_BOOLEAN_FLAGS {
            let message = error_of(&[
                "changes", "page", "--run-id", run, "--cursor", flag, "--json",
            ]);
            assert!(
                message.contains("--cursor requires a value") && message.contains("boolean flag"),
                "reserved {flag} was not rejected: {message}"
            );
        }
        let prompt = error_of(&["run", "--prompt", "--json"]);
        assert!(prompt.contains("--prompt requires a value"), "{prompt}");
        let accounting = parse_accounting_args(&[
            "accounting".to_owned(),
            "inspect".to_owned(),
            "--workspace".to_owned(),
            "--json".to_owned(),
            "--database".to_owned(),
            r"C:\fixture\state.db".to_owned(),
            "--session-id".to_owned(),
            "01234567-89ab-cdef-0123-456789abcdef".to_owned(),
            "--goal-id".to_owned(),
            "01234567-89ab-cdef-0123-456789abcdef".to_owned(),
            "--request-id".to_owned(),
            "01234567-89ab-cdef-0123-456789abcdef".to_owned(),
        ])
        .unwrap_err();
        assert!(
            accounting.contains("--workspace needs a value") && accounting.contains("boolean flag"),
            "{accounting}"
        );
        assert!(matches!(
            parse_args(["run", "--prompt", "ordinary task", "--json"]),
            Ok(Arguments::Run { prompt, json: true, .. }) if prompt == "ordinary task"
        ));
    }
}

fn parse_accounting_args(values: &[String]) -> Result<Arguments, String> {
    let mode = values
        .get(1)
        .map(String::as_str)
        .ok_or("accounting requires inspect or resolve")?;
    let mut fields = std::collections::BTreeMap::new();
    let mut json = false;
    let mut isolated = false;
    let mut index = 2;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--json" {
            if json {
                return Err("duplicate --json".into());
            }
            json = true;
            index += 1;
            continue;
        }
        if flag == "--isolated" {
            if isolated {
                return Err("duplicate --isolated".into());
            }
            isolated = true;
            index += 1;
            continue;
        }
        if flag == "--no-color" {
            index += 1;
            continue;
        }
        if !matches!(
            flag.as_str(),
            "--workspace"
                | "--database"
                | "--session-id"
                | "--goal-id"
                | "--request-id"
                | "--resolution"
                | "--confirm-reservation"
        ) {
            return Err(format!("unsupported accounting option: {flag}"));
        }
        let value = values
            .get(index + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        if RESERVED_BOOLEAN_FLAGS.contains(&value.as_str()) {
            return Err(format!(
                "{flag} needs a value; {value} is a boolean flag and cannot be consumed as one"
            ));
        }
        if fields.insert(flag.as_str(), value.as_str()).is_some() {
            return Err(format!("duplicate {flag}"));
        }
        index += 2;
    }
    let required = |key| {
        fields
            .get(key)
            .copied()
            .ok_or_else(|| format!("accounting requires {key}"))
    };
    let workspace = PathBuf::from(required("--workspace")?);
    let database = PathBuf::from(required("--database")?);
    if !workspace.is_absolute() || !database.is_absolute() {
        return Err("accounting requires absolute workspace and database paths".into());
    }
    let action = match mode {
        "inspect"
            if !fields.contains_key("--resolution")
                && !fields.contains_key("--confirm-reservation") =>
        {
            AccountingAction::Inspect {
                session_id: argentum_domain::SessionId::parse_str(required("--session-id")?)
                    .map_err(|_| "invalid session ID")?,
                goal_id: argentum_domain::GoalId::parse_str(required("--goal-id")?)
                    .map_err(|_| "invalid goal ID")?,
                request_id: argentum_domain::RunId::parse_str(required("--request-id")?)
                    .map_err(|_| "invalid physical request ID")?,
            }
        }
        "resolve"
            if !fields.contains_key("--session-id")
                && !fields.contains_key("--goal-id")
                && !fields.contains_key("--request-id") =>
        {
            AccountingAction::Resolve {
                resolution_file: PathBuf::from(required("--resolution")?),
                confirm_reservation: fields
                    .get("--confirm-reservation")
                    .map(|v| parse_u64_option(v, "--confirm-reservation"))
                    .transpose()?,
            }
        }
        _ => return Err("incompatible accounting options or action".into()),
    };
    Ok(Arguments::Accounting {
        options: CommonOptions {
            workspace,
            database: Some(database),
            no_color: true,
            isolated,
            ..CommonOptions::default()
        },
        action,
        json,
    })
}

async fn run_accounting_command(
    options: CommonOptions,
    action: AccountingAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use argentum_domain::goal_accounting::{AccountingDecision, AccountingResolution};
    let database = options
        .database
        .as_ref()
        .ok_or("explicit database is required")?;
    match action {
        AccountingAction::Inspect {
            session_id,
            goal_id,
            request_id,
        } => {
            let inspection = Store::inspect_accounting_recovery(
                database,
                &options.workspace,
                session_id,
                goal_id,
                request_id,
            )?;
            if json {
                println!("{}", serde_json::to_string(&inspection)?);
            } else {
                println!("Request: {}\nReservation: {}\nRecoverable: {}\nProvider usage complete: {}\nBlocker: {}",request_id,inspection.charge.reserved_tokens,inspection.recoverable,inspection.provider_usage_complete,inspection.blocker.as_deref().unwrap_or("none"));
            }
        }
        AccountingAction::Resolve {
            resolution_file,
            confirm_reservation,
        } => {
            let mut file = tokio::fs::File::open(resolution_file).await?;
            let mut bytes = Vec::new();
            (&mut file)
                .take(16 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() > 16 * 1024 {
                return Err("accounting resolution exceeds16 KiB".into());
            }
            let resolution: AccountingResolution =
                serde_json::from_slice(&bytes).map_err(|_| "invalid accounting resolution JSON")?;
            match &resolution.decision {
                AccountingDecision::OwnerAcceptReservation if confirm_reservation==Some(resolution.binding.reserved_tokens)=>{}
                AccountingDecision::AuthoritativeUsage{..} if confirm_reservation.is_none()=>{}
                _=>return Err("owner acceptance requires --confirm-reservation matching the full exact reservation; authoritative usage cannot use this confirmation".into()),
            }
            let preflight = Store::inspect_accounting_recovery(
                database,
                &options.workspace,
                resolution.binding.session_id,
                resolution.binding.goal_id,
                resolution.binding.request_id,
            )?;
            let b = &resolution.binding;
            if preflight.binding.store_id != b.store_id
                || preflight.binding.physical_store_sha256 != b.physical_store_sha256
                || preflight.binding.project_id != b.project_id
                || preflight.binding.workspace_key != b.workspace_key
                || preflight.binding.run_id != b.run_id
                || preflight.binding.owner_generation != b.owner_generation
                || preflight.binding.owner_id != b.owner_id
                || preflight.binding.reserved_tokens != b.reserved_tokens
            {
                return Err("accounting recovery scope or physical store mismatch".into());
            }
            // Like verification approve and goal limits, this is a direct local
            // owner command, not model/JSONL authority or a physical-key claim.
            let config = options.host_config()?;
            with_one_shot_host(config,move |host|Box::pin(async move {
                let charge=host.local_handle().resolve_accounting_recovery(&resolution)?;
                if json { println!("{}",serde_json::json!({"command_status":"accepted_and_settled","charge":charge,"provider_usage_complete":charge.disposition.as_ref().is_some_and(|d|d.later_observed_usage.is_some() && d.conflicting_later_usage.is_empty()),"inference_dispatched":false,"verifier_dispatched":false})); }
                else { println!("Accounting disposition recorded for {}. Original outcome and usage uncertainty retained; no work started.",charge.request_id); }
                Ok(())
            })).await?;
        }
    }
    Ok(())
}

fn parse_recovery_args(values: &[String]) -> Result<Arguments, String> {
    let action = values
        .get(1)
        .ok_or_else(|| "recovery requires 'inspect', 'reconcile', or 'review'".to_owned())?;
    if !matches!(action.as_str(), "inspect" | "reconcile" | "review") {
        return Err("recovery requires 'inspect', 'reconcile', or 'review'".into());
    }
    let mut database = None;
    let mut run_id = None;
    let mut revision = None;
    let mut json = false;
    let mut index = 2;
    while index < values.len() {
        match values[index].as_str() {
            "--database" => {
                if database.is_some() {
                    return Err("--database may be supplied once".into());
                }
                index += 1;
                database = Some(PathBuf::from(required_value(values, index, "--database")?));
            }
            "--run-id" if action == "review" => {
                if run_id.is_some() {
                    return Err("--run-id may be supplied once".into());
                }
                index += 1;
                run_id = Some(
                    required_value(values, index, "--run-id")?
                        .parse()
                        .map_err(|_| "--run-id requires an exact UUID".to_owned())?,
                );
            }
            "--revision" if action == "review" => {
                if revision.is_some() {
                    return Err("--revision may be supplied once".into());
                }
                index += 1;
                revision = Some(
                    required_value(values, index, "--revision")?
                        .parse()
                        .map_err(|_| "--revision requires an exact UUID".to_owned())?,
                );
            }
            "--json" if !json => json = true,
            value => return Err(format!("unexpected recovery argument '{value}'")),
        }
        index += 1;
    }
    let database = database.unwrap_or(
        AppPaths::discover()
            .map_err(|error| error.to_string())?
            .database,
    );
    let action = match action.as_str() {
        "inspect" => RecoveryAction::Inspect,
        "reconcile" => RecoveryAction::Reconcile,
        "review" => RecoveryAction::Review {
            run_id: run_id.ok_or_else(|| "recovery review requires --run-id".to_owned())?,
            revision: revision.ok_or_else(|| "recovery review requires --revision".to_owned())?,
        },
        _ => unreachable!(),
    };
    Ok(Arguments::Recovery {
        database,
        action,
        json,
    })
}

fn required_value<'a>(values: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    let value = values
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))?;
    if RESERVED_BOOLEAN_FLAGS.contains(&value) {
        return Err(format!(
            "{option} requires a value; {value} is a boolean flag and cannot be consumed as one"
        ));
    }
    Ok(value)
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
        "llama-cpp" => Ok(argentum_domain::ProviderKind::LocalLlamaCpp),
        "lm-studio" => Ok(argentum_domain::ProviderKind::LocalLmStudio),
        "openai-compatible" => Ok(argentum_domain::ProviderKind::OpenAiCompatible),
        "anthropic" => Ok(argentum_domain::ProviderKind::Anthropic),
        _ => Err("--kind must be llama-cpp, lm-studio, openai-compatible, or anthropic".to_owned()),
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
        "trajectory" => Ok(argentum_domain::SurfaceId::Trajectory),
        "approvals" => Ok(argentum_domain::SurfaceId::Approvals),
        _ => Err(
            "SURFACE must be conversation, plan, changes, files, terminal, preview, activity, trajectory, or approvals"
                .to_owned(),
        ),
    }
}

fn parse_verification_runner(value: &str) -> Result<argentum_domain::VerificationRunnerId, String> {
    match value {
        "cargo-fmt-check" => Ok(argentum_domain::VerificationRunnerId::CargoFmtCheck),
        "cargo-check-locked" => Ok(argentum_domain::VerificationRunnerId::CargoCheckLocked),
        "cargo-test-locked" => Ok(argentum_domain::VerificationRunnerId::CargoTestLocked),
        "cargo-clippy-strict" => Ok(argentum_domain::VerificationRunnerId::CargoClippyStrict),
        value
            if value
                .strip_prefix("project:")
                .is_some_and(|id| !id.is_empty()) =>
        {
            Ok(argentum_domain::VerificationRunnerId::Project(
                value
                    .strip_prefix("project:")
                    .unwrap_or_default()
                    .to_owned(),
            ))
        }
        _ => Err("runner accepts a built-in ID or project:ID".to_owned()),
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

fn task_command(prompt: String, skill: Option<String>) -> AppCommand {
    match skill {
        Some(skill) => AppCommand::SubmitTaskWithSkill { prompt, skill },
        None => AppCommand::SubmitTask { prompt },
    }
}

fn skill_summary_json(summary: &argentum_workspaces::SkillSummary) -> serde_json::Value {
    serde_json::json!({
        "name": summary.name,
        "description": summary.description,
        "path": summary.relative_path,
        "base_path": summary.base_path,
        "digest": summary.digest,
        "valid": summary.valid,
        "error": summary.error,
    })
}

fn print_skill_summary(summary: &argentum_workspaces::SkillSummary) {
    println!(
        "- {}  {}  sha256 {}  {}",
        summary.name.as_deref().unwrap_or("<invalid>"),
        if summary.valid { "valid" } else { "invalid" },
        summary.digest.as_deref().unwrap_or("unavailable"),
        summary
            .error
            .as_deref()
            .unwrap_or(summary.relative_path.as_str())
    );
}

const HELP_BODY: &str = "Argentum Genesis command host\n\n\
Usage:\n  \
argentum\n  \
argentum chat [OPTIONS]\n  \
argentum setup [OPTIONS]\n  \
argentum doctor [OPTIONS]\n  \
argentum serve [OPTIONS]\n  \
argentum run [OPTIONS] [--goal GOAL_ID] --prompt TEXT [--skill NAME]\n  \
argentum run --final-result [--answer-schema RELATIVE_FILE] --prompt TEXT [OPTIONS]\n  \
argentum result RUN_ID [OPTIONS]\n  \
argentum diagnostics RUN_ID [--json] [OPTIONS]\n  \
argentum accounting inspect --workspace PATH --database PATH --session-id SESSION --goal-id GOAL --request-id REQUEST [--json]\n  \
argentum accounting resolve --workspace PATH --database PATH --resolution FILE [--confirm-reservation EXACT_UNITS] [--json]\n  \
argentum skills list [OPTIONS]\n  \
argentum skills inspect NAME [OPTIONS]\n  \
argentum status [OPTIONS]\n  \
argentum provider probe [PROVIDER_ID] [OPTIONS]\n  \
argentum provider list [OPTIONS]\n  \
argentum terminal-diagnostic --synthetic --output ABSOLUTE_NEW_FILE\n  \
argentum local-chat [OPTIONS]  (llama.cpp coding with policy-controlled tools)\n  \
argentum local-text [OPTIONS]  (explicit text-only mode, no tools)\n  \
argentum provider save PROFILE_ID --label LABEL --kind KIND --endpoint URL --model NAME [--select] [OPTIONS]\n  \
argentum provider add minimax|deepseek|openai|chatgpt [OPTIONS]\n  \
argentum provider login chatgpt [OPTIONS]  (browser consent, no API key)\n  \
argentum provider login-new chatgpt [OPTIONS]\n  \
argentum provider accounts chatgpt [OPTIONS]\n  \
argentum provider account chatgpt ACCOUNT_ID [OPTIONS]\n  \
argentum provider logout chatgpt [OPTIONS]\n  \
argentum provider select PROFILE_ID [OPTIONS]\n  \
argentum provider models PROFILE_ID [OPTIONS]\n  \
argentum provider model PROFILE_ID --model NAME [OPTIONS]\n  \
argentum provider budget PROFILE_ID --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY [OPTIONS]\n  \
argentum provider context llama-cpp [--size 8k|16k|32k|64k|128k|256k] [OPTIONS]\n  \
argentum context --json [OPTIONS]\n  \
argentum instructions [--directory RELATIVE_DIRECTORY] [OPTIONS]\n  \
argentum compact [--deactivate | --goal GOAL_ID] --json [OPTIONS]\n  \
argentum provider credential set PROFILE_ID < credential.txt\n  \
argentum provider credential clear PROFILE_ID [OPTIONS]\n  \
argentum workspace status [OPTIONS]\n  \
argentum workspace set PATH [OPTIONS]\n  \
argentum sessions [OPTIONS]\n  \
argentum session select SESSION_ID [OPTIONS]\n  \
argentum session inspect|pin|unpin|archive|unarchive SESSION_ID [OPTIONS]\n  \
argentum session title SESSION_ID TITLE [OPTIONS]\n  \
argentum session fork SOURCE_SESSION_ID --title TITLE [OPTIONS]\n  \
argentum goal status [OPTIONS]\n  \
argentum goal limits --goal-id GOAL_ID (--limits GOAL_TOKENS,REQUESTS,ITERATIONS,TOOLS,SECONDS,RUN_TOKENS,RUN_REQUESTS,RUN_TOOLS,RUN_SECONDS | --remove-aggregate) [OPTIONS]\n  \
argentum goal criteria --goal-id GOAL_ID --runners RUNNER[,RUNNER] [--owner-attestation-required] [OPTIONS]\n  \
argentum goal attest --goal-id GOAL_ID --summary TEXT [OPTIONS]\n  \
argentum goal supervise --goal-id GOAL_ID --iterations N --elapsed-seconds N [OPTIONS]\n  \
argentum goal deliver --goal-id GOAL_ID --iterations N --elapsed-seconds N [OPTIONS]\n  \
argentum goal set OBJECTIVE [--token-budget N] [--tool-budget N] [--time-budget SECONDS] [OPTIONS]\n  \
argentum goal pause [OPTIONS]\n  \
argentum goal resume [OPTIONS]\n  \
argentum goal clear [OPTIONS]\n  \
argentum harness status [OPTIONS]\n  \
argentum harness profile PROFILE_ID [OPTIONS]\n  \
argentum harness surface SURFACE show|hide [OPTIONS]\n  \
argentum harness execution PROFILE_ID [OPTIONS]\n  \
argentum harness capability CAPABILITY_ID enable|disable [OPTIONS]\n  \
argentum harness backend enable|disable [OPTIONS]\n  \
argentum harness module MODULE_ID install|enable|disable|remove [OPTIONS]\n  \
argentum harness trajectory [SESSION_ID] [OPTIONS]\n  \
argentum tool list [OPTIONS]\n  \
argentum permissions list [OPTIONS]\n  \
argentum permissions select PROFILE_ID [OPTIONS]\n  \
argentum permissions grants [OPTIONS]\n  \
argentum permissions revoke GRANT_ID [OPTIONS]\n  \
argentum verification list [OPTIONS]\n  \
argentum verification inspect [OPTIONS]\n  \
argentum verification approve RUNNER [OPTIONS]\n  \
argentum verification status [RUN_ID] [OPTIONS]\n\n\
argentum changes page --run-id UUID [--limit N] [--cursor TEXT] --json\n  \
argentum changes list RUN_ID [OPTIONS]\n  \
argentum changes diff FILE_CHANGE_ID [OPTIONS]\n  \
argentum changes restore FILE_CHANGE_ID [OPTIONS]\n  \
argentum changes recovery [retry|restore OPERATION_ID] [OPTIONS]\n\n\
argentum recovery inspect [--database PATH] [--json]\n  \
argentum recovery reconcile [--database PATH] [--json]\n  \
argentum recovery review --run-id RUN_ID --revision REVISION [--database PATH] [--json]\n\n\
Commands:\n  \
no command  Open the interactive terminal shell\n  \
chat        Open the interactive terminal shell\n  \
  setup       Run deterministic first-run onboarding\n  \
doctor      Diagnostics, planned for a later phase\n  \
serve       Keep one runtime alive and exchange protocol v1 JSONL on stdin/stdout\n  \
  run         Submit one task and stream its output; --final-result emits one settled result\n  \
  result      Read one settled run result without launching work\n  \
  diagnostics Read bounded metadata for one run without launching work\n  \
  skills      List or inspect project-local .agents/skills resources\n  \
status      Show workspace and configured provider information\n  \
provider    Manage and test workspace provider profiles\n  \
workspace   Manage the persisted workspace selection\n  \
sessions    List durable sessions for the current workspace\n  \
session     Select the active session for the current workspace\n  \
goal        Manage the active session goal contract\n  \
harness     Inspect composition, presentation, execution policy, and surfaces\n  \
tool        List registered developer tools and current availability\n  \
instructions Preview selected project instruction metadata (--directory RELATIVE_DIRECTORY)\n  \
context     Preview selected working-context metadata; no tool bodies or token estimates\n  \
permissions Inspect profiles, select local authority, and manage grants\n  \
verification List fixed runners and inspect freshness-bound evidence\n  \
  changes     Inspect durable changes, load a diff, or request approved restore\n\n\
  recovery    Inspect or explicitly reconcile abandoned local owner state\n\n\
Options:\n  \
--workspace PATH   Workspace boundary, defaults to the current directory\n  \
--database PATH    SQLite database path, defaults to Argentum application data\n  \
--isolated         Use explicit workspace/database only; no saved credentials or local runtime\n  \
--endpoint URL     Legacy HTTP endpoint field; local inference uses llama.cpp\n  \
  --model NAME       Provider model name\n  \
  --skill NAME       Select one project-local skill for this run only\n  \
  --final-result     Emit one bounded settled result JSON document on stdout\n  \
  --answer-schema FILE  Validate the terminal model answer against one bounded workspace schema; requires --final-result\n  \
  --answer-schema-sha256 SHA256  Optionally require the selected schema file digest\n  \
--label LABEL      Display label for provider save\n  \
--kind KIND        llama-cpp, lm-studio, openai-compatible, or anthropic\n  \
--select           Select a profile when saving it\n  \
--objective TEXT   Goal objective when using 'goal set'\n  \
--token-budget N   Maximum model tokens for the goal contract\n  \
--tool-budget N    Maximum tool calls for the goal contract\n  \
--time-budget N    Maximum goal duration in seconds\n  \
--remove-aggregate  Explicit owner amendment for 'goal limits'; keeps charges and finite per-run safeguards\n  \
--runners LIST     Fixed typed verification runners for 'goal criteria'\n  \
--iterations N     Finite attempt cap for 'goal supervise', maximum 16\n  \
--elapsed-seconds N Overall invocation deadline for 'goal supervise', maximum 86400\n  \
--json             Emit machine-readable output for supported inspection commands; ordinary run remains JSONL\n  \
--jsonl            Use the JSONL command server\n  \
--no-color         Disable terminal color\n  \
--non-interactive  Use the plain line-oriented terminal fallback\n  \
provider credential set reads the secret from stdin and never accepts it as an option\n  \
-V, --version      Print the Argentum version";

fn help_text() -> String {
    let mut text = String::new();
    #[cfg(feature = "extensions")]
    text.push_str("Optional governed extensions: argentum extensions help. Install/configure is not execution approval; activation requires the local owner console.\n");
    #[cfg(feature = "artifacts")]
    text.push_str("Optional local document artifacts: argentum artifacts help. Inspect, serve and verify require an exact owner configuration; serving is not model authority.\n");
    text.push_str(HELP_BODY);
    text.push('\n');
    text
}

fn print_help() {
    print!("{}", help_text());
}

fn print_version() {
    println!("{}", version_line());
}

fn version_line() -> String {
    format!("argentum {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use argentum_cli::protocol::RequestEnvelope;

    use super::*;

    #[tokio::test]
    async fn changes_page_command_preserves_retained_journal_and_workspace() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("journal.sqlite3");
        let run = argentum_domain::RunId::new_v4();
        let store = argentum_store::Store::open(&database).unwrap();
        let scope = store.resolve_workspace(root.path()).unwrap();
        let operation_id = argentum_domain::ChangeOperationId::new_v4();
        store
            .prepare_change_operation(&argentum_store::PreparedChangeOperation {
                operation_id,
                workspace_key: scope.workspace_key.clone(),
                project_id: scope.snapshot.project.id,
                session_id: scope.snapshot.active_session_id.unwrap(),
                run_id: run,
                relative_path: "historical-only.txt".into(),
                kind: argentum_domain::FileChangeKind::Added,
                origin: argentum_domain::FileChangeOrigin::ToolCall,
                before: None,
                after: Some(argentum_domain::FileSnapshotRef {
                    digest: argentum_domain::Sha256Digest(
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                    ),
                    size_bytes: 0,
                    retained: true,
                }),
                before_bytes: None,
                after_bytes: Some(Vec::new()),
                additions: 0,
                removals: 0,
                line_count_availability: argentum_domain::LineCountAvailability::Unknown,
                restored_from: None,
            })
            .unwrap();
        drop(store);
        std::fs::write(
            root.path().join("preserved-source.rs"),
            b"// retained source\n",
        )
        .unwrap();
        let files = || {
            let mut files = std::fs::read_dir(root.path())
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.unwrap().path();
                    let name = path.file_name().unwrap().to_owned();
                    // A normal read-only WAL connection can create coordination files.
                    // SHM contains read locks, not journal records. This fixture has
                    // no concurrent writer: any WAL frames would violate preservation.
                    if name == "journal.sqlite3-shm" {
                        assert_eq!(std::fs::metadata(&path).unwrap().len(), 32768);
                        return None;
                    }
                    if name == "journal.sqlite3-wal" {
                        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
                        return None;
                    }
                    Some((name, std::fs::read(&path).unwrap()))
                })
                .collect::<Vec<_>>();
            files.sort_by(|a, b| a.0.cmp(&b.0));
            files
        };
        let before = files();
        for json in [true, false] {
            let bound = argentum_domain::ChangePageCursor {
                project_id: scope.snapshot.project.id,
                session_id: scope.snapshot.active_session_id.unwrap(),
                run_id: run,
                stamp: 0,
                sequence: 0,
                id: operation_id,
            };
            for foreign in [
                argentum_domain::ChangePageCursor {
                    project_id: argentum_domain::ProjectId::new_v4(),
                    ..bound.clone()
                },
                argentum_domain::ChangePageCursor {
                    session_id: argentum_domain::SessionId::new_v4(),
                    ..bound.clone()
                },
                argentum_domain::ChangePageCursor {
                    run_id: argentum_domain::RunId::new_v4(),
                    ..bound.clone()
                },
            ] {
                assert!(run_changes_command(
                    CommonOptions {
                        workspace: root.path().to_path_buf(),
                        database: Some(database.clone()),
                        ..CommonOptions::isolated()
                    },
                    ChangeAction::Page {
                        run_id: run,
                        limit: 1,
                        cursor: Some(serde_json::to_string(&foreign).unwrap()),
                    },
                    json,
                )
                .await
                .is_err());
                assert!(
                    files() == before,
                    "rejected cursor changed retained storage"
                );
            }
            run_changes_command(
                CommonOptions {
                    workspace: root.path().to_path_buf(),
                    database: Some(database.clone()),
                    ..CommonOptions::isolated()
                },
                ChangeAction::Page {
                    run_id: run,
                    limit: 1,
                    cursor: None,
                },
                json,
            )
            .await
            .unwrap();
            assert!(files() == before, "inspection changed retained storage");
            assert!(!root.path().join("historical-only.txt").exists());
            let store = argentum_store::Store::open_existing_read_only(&database).unwrap();
            let page = store
                .change_page(
                    scope.snapshot.project.id,
                    scope.snapshot.active_session_id.unwrap(),
                    run,
                    1,
                    None,
                )
                .unwrap();
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].operation_id, Some(operation_id));
            assert_eq!(page.items[0].file_change_id, None);
            assert_eq!(page.items[0].id, operation_id);
            assert_eq!(page.items[0].path, "historical-only.txt");
            assert_eq!(page.items[0].kind, "created");
            assert_eq!(page.items[0].state, "prepared");
            assert!(page.complete);
            assert!(page.next_cursor.is_none());
            assert!(files() == before, "journal read changed retained storage");
        }
    }

    #[test]
    fn f21a_diagnostics_parser_requires_run_id_and_json_is_explicit() {
        let run = argentum_domain::RunId::new_v4();
        assert!(matches!(
            parse_args(["diagnostics", &run.to_string(), "--json"]).unwrap(),
            Arguments::Diagnostics { run_id, json: true, .. } if run_id == run
        ));
        assert!(matches!(
            parse_args(["diagnostics", &run.to_string()]).unwrap(),
            Arguments::Diagnostics { run_id, json: false, .. } if run_id == run
        ));
        assert!(parse_args(["diagnostics"]).is_err());
        assert!(parse_args(["diagnostics", "not-a-run"]).is_err());
    }

    #[test]
    fn local_entry_names_distinguish_coding_from_explicit_text_only() {
        assert!(matches!(
            parse_args(["local-chat"]).unwrap(),
            Arguments::LocalChat {
                text_only: false,
                ..
            }
        ));
        assert!(matches!(
            parse_args(["local-text"]).unwrap(),
            Arguments::LocalChat {
                text_only: true,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn configured_local_setup_selects_coding_and_uncapped_context_bound_budget() {
        let root = tempfile::tempdir().unwrap();
        let config = HostConfig::in_memory(root.path()).with_local_runtime(
            argentum_providers::LocalRuntimeConfig::new(
                root.path().join("unused-bundle.zip"),
                root.path().join("models"),
            )
            .unwrap(),
        );
        let mut host = start_active_host(config).await.unwrap();
        let state = SetupState::new(OnboardingSnapshot::initial());
        for action in ["local", "llama-cpp"] {
            let input = format!("{action}\n");
            let mut reader = BufReader::new(input.as_bytes());
            let mut writer = Vec::new();
            let choice = prompt_provider_with_credentials(
                &host,
                &state,
                &mut reader,
                &mut writer,
                false,
                &mut |_| panic!("local coding requested a key"),
            )
            .await
            .unwrap();
            assert!(
                matches!(choice,ProviderChoice::Coding(AppCommand::SetOnboardingProvider{provider_id}) if provider_id=="llama-cpp")
            );
            let output = String::from_utf8(writer).unwrap();
            assert!(output.contains("[coding supported]"));
            assert!(output.contains("Use llama.cpp coding (tools enabled"));
            assert!(!output.contains("text-only"));
            assert!(!output.contains("no coding tools"));
        }
        assert!(ensure_local_coding_budget(&host).unwrap());
        let budget = host.selected_model_budget().unwrap().unwrap();
        assert_eq!(
            (
                budget.usable_context_tokens,
                budget.maximum_output_tokens,
                budget.requested_output_tokens
            ),
            (8192, 8191, 0)
        );
        assert_eq!(host.execution_policy().unwrap().profile_id, "guided");
        assert!(!host.local_runtime_active("Qwen3.5-0.8B-Q8_0.gguf").await);
        host.shutdown().await.unwrap();
    }

    #[derive(Default)]
    struct FixtureSecrets(Mutex<std::collections::BTreeMap<String, SecretValue>>);
    impl argentum_platform::SecretStore for FixtureSecrets {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, argentum_platform::PlatformError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn set(
            &self,
            key: &str,
            value: SecretValue,
        ) -> Result<(), argentum_platform::PlatformError> {
            self.0.lock().unwrap().insert(key.into(), value);
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), argentum_platform::PlatformError> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "explicit synthetic terminal and temporary OS secret-store check"]
    async fn f20c_entry_native_hidden_setup_round_trip() {
        use argentum_platform::{OsSecretStore, SecretStore};
        assert!(io::stdin().is_terminal() && io::stdout().is_terminal());
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(OsSecretStore::new(format!(
            "com.argentum.synthetic-entry-{}",
            argentum_domain::RunId::new_v4()
        )));
        struct Cleanup(Arc<OsSecretStore>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete("provider/minimax");
            }
        }
        let _cleanup = Cleanup(store.clone());
        assert!(store.get("provider/minimax").unwrap().is_none());
        let mut host = start_active_host(
            HostConfig::in_memory(directory.path()).with_secret_store(store.clone()),
        )
        .await
        .unwrap();
        let state = SetupState::new(OnboardingSnapshot::initial());
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut writer = tokio::io::stdout();
        println!("SYNTHETIC ONLY. Use minimax, y, and the synthetic key supplied by the test operator. No provider request is made. The separate OS test entry is deleted afterward.");
        let choice = prompt_provider_with_credentials(
            &host,
            &state,
            &mut reader,
            &mut writer,
            true,
            &mut prompt_provider_secret,
        )
        .await
        .unwrap();
        assert!(matches!(choice, ProviderChoice::Coding(_)));
        let saved = store.get("provider/minimax").unwrap().unwrap();
        assert!(
            saved.expose() == "F20C-SYNTHETIC-ENTRY-NOT-REAL",
            "secret did not match the synthetic fixture; value withheld"
        );
        let reply = read_setup_line(
            &mut reader,
            &mut writer,
            "Echo restored. Type ready and Enter: ",
        )
        .await
        .unwrap();
        assert_eq!(reply, "ready");
        host.shutdown().await.unwrap();
        store.delete("provider/minimax").unwrap();
        assert!(store.get("provider/minimax").unwrap().is_none());
        println!(
            "Synthetic hidden input, secure-store round-trip, deletion and restored input passed."
        );
    }

    #[tokio::test]
    async fn f20c_entry_key_cancel_keeps_setup_open_then_saves_without_disclosure() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = start_active_host(
            HostConfig::in_memory(directory.path())
                .with_secret_store(Arc::new(FixtureSecrets::default())),
        )
        .await
        .unwrap();
        let mut state = SetupState::new(OnboardingSnapshot::initial());
        state
            .apply(argentum_cli::onboarding::SetupAction::SelectGuidance {
                level: GuidanceLevel::Guided,
            })
            .unwrap();
        let mut reader = BufReader::new(&b"minimax\ny\nminimax\ny\n"[..]);
        let mut writer = Vec::new();
        let mut calls = 0;
        let choice = prompt_provider_with_credentials(
            &host,
            &state,
            &mut reader,
            &mut writer,
            true,
            &mut |id| {
                assert_eq!(id, "minimax");
                calls += 1;
                if calls == 1 {
                    Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "synthetic cancellation",
                    ))
                } else {
                    Ok(SecretValue::new("F20C-SYNTHETIC-KEY-NEVER-REAL"))
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(calls, 2);
        assert!(
            matches!(choice, ProviderChoice::Coding(AppCommand::SetOnboardingProvider { provider_id }) if provider_id == "minimax")
        );
        assert!(host.provider_credential_configured("minimax"));
        let output = String::from_utf8(writer).unwrap();
        assert!(output.contains("setup is still open") && output.contains("Key saved securely"));
        assert!(!output.contains("F20C-SYNTHETIC-KEY"));
        assert!(!serde_json::to_string(&host.onboarding_snapshot().unwrap())
            .unwrap()
            .contains("F20C-SYNTHETIC-KEY"));
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn f20c_entry_local_alias_is_visible_selected_and_does_not_prompt_for_keys() {
        for action in ["local", "llama-cpp", ""] {
            let directory = tempfile::tempdir().unwrap();
            let mut host = start_active_host(HostConfig::in_memory(directory.path()))
                .await
                .unwrap();
            let state = SetupState::new(OnboardingSnapshot::initial());
            let input = format!("{action}\ny\n");
            let mut reader = BufReader::new(input.as_bytes());
            let mut writer = Vec::new();
            let choice = prompt_provider_with_credentials(
                &host,
                &state,
                &mut reader,
                &mut writer,
                false,
                &mut |_| panic!("local route asked for a key"),
            )
            .await
            .unwrap();
            assert!(matches!(choice, ProviderChoice::LocalChat));
            assert!(selected_profile_needs_local_text(&host).unwrap());
            let output = String::from_utf8(writer).unwrap();
            assert!(
                output.contains("[llama-cpp]")
                    && output.contains("local text chat; no coding tools")
            );
            assert!(!output.contains("tool-template and parser contract"));
            host.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn f20c_entry_completed_setup_preserves_the_selected_text_only_route() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.db");
        {
            let store = Store::open(&database).unwrap();
            let scope = store.resolve_workspace(directory.path()).unwrap().snapshot;
            store
                .save_onboarding(
                    scope.project.id,
                    &OnboardingSnapshot::legacy_complete(
                        "llama-cpp",
                        "Qwen3.5-0.8B-Q8_0.gguf",
                        scope.project.workspace_root,
                        "guided",
                    ),
                )
                .unwrap();
        }
        let mut config = HostConfig::in_memory(directory.path());
        config.database = Some(database);
        let host = start_active_host(config).await.unwrap();
        let mut reader = BufReader::new(&b""[..]);
        let mut writer = Vec::new();
        let options = CommonOptions::default();
        let mut prepared =
            ensure_onboarding_with_io(host, options, &mut reader, &mut writer, false)
                .await
                .unwrap();
        assert!(prepared.local_chat);
        assert!(writer.is_empty(), "completed setup restarted the wizard");
        prepared.host.shutdown().await.unwrap();
    }

    #[test]
    fn synthetic_diagnostic_requires_explicit_arming_and_no_host_options() {
        assert!(matches!(
            parse_args([
                "terminal-diagnostic",
                "--synthetic",
                "--output",
                "C:\\temp\\new-trace.jsonl"
            ])
            .unwrap(),
            Arguments::TerminalDiagnostic { .. }
        ));
        assert!(parse_args([
            "terminal-diagnostic",
            "--output",
            "C:\\temp\\new-trace.jsonl"
        ])
        .is_err());
        assert!(parse_args([
            "terminal-diagnostic",
            "--synthetic",
            "--output",
            "C:\\temp\\new-trace.jsonl",
            "--workspace",
            "C:\\private"
        ])
        .is_err());
    }

    #[test]
    fn staging_artifact_directory_is_not_used_as_the_implicit_workspace() {
        let staging = PathBuf::from(r"C:\argentum\artifacts\windows-cli\20260912-175637");
        assert_eq!(
            avoid_staging_artifact_workspace(staging),
            PathBuf::from(r"C:\argentum")
        );
        let ordinary = PathBuf::from(r"C:\projects\fixture");
        assert_eq!(avoid_staging_artifact_workspace(ordinary.clone()), ordinary);
    }

    #[test]
    fn machine_and_noninteractive_modes_cannot_claim_direct_local_user_authority() {
        assert!(direct_local_user_interaction(false, false, true, true));
        assert!(!direct_local_user_interaction(true, false, true, true));
        assert!(!direct_local_user_interaction(false, true, true, true));
        assert!(!direct_local_user_interaction(false, false, false, true));
        assert!(!direct_local_user_interaction(false, false, true, false));
    }

    #[test]
    fn local_runtime_launch_plan_displays_the_digest_bound_model_maximum() {
        let mut plan = LocalRuntimeLaunchPlan {
            runtime_id: "llama-cpp".into(),
            runtime_version: "v0.3.0".into(),
            runtime_bundle_sha256: "a".repeat(64),
            executable_sha256: "b".repeat(64),
            program: PathBuf::from(r"C:\runtime\llama-server.exe"),
            arguments: vec!["--offline".into()],
            environment_sha256: argentum_domain::Sha256Digest("c".repeat(64)),
            model: "model.gguf".into(),
            model_path: PathBuf::from(r"C:\models\model.gguf"),
            model_sha256: "d".repeat(64),
            model_size_bytes: 4,
            max_model_bytes: 64 * 1024 * 1024,
            managed_active_process_limit: 4,
            managed_cpu_rate_percent: 95,
            managed_process_memory_bytes: 4 * 1024 * 1024 * 1024,
            managed_job_memory_bytes: 4 * 1024 * 1024 * 1024,
            managed_output_hash_limit: 4 * 1024 * 1024,
            generation: 1,
            epoch: "epoch".into(),
            host: "127.0.0.1".into(),
            port: 43_123,
            api_prefix: "/argentum/generation-1/epoch".into(),
            context_tokens: 8_192,
            gpu_layers: 99,
            disabled_features: vec!["web-ui".into()],
        };

        let first_digest = plan.digest();
        let first_display = format_local_runtime_launch_plan(&plan);
        assert!(first_display.contains("Model maximum bytes: 67108864"));
        assert!(first_display.contains("Managed active process limit: 4"));
        assert!(first_display.contains("Managed CPU rate percent: 95"));
        assert!(first_display.contains("Managed process memory bytes: 4294967296"));
        assert!(first_display.contains("Managed job memory bytes: 4294967296"));
        assert!(first_display.contains("Managed output hash bytes per stream: 4194304"));

        plan.max_model_bytes = 128 * 1024 * 1024;
        assert_ne!(plan.digest(), first_digest);
        assert!(format_local_runtime_launch_plan(&plan).contains("Model maximum bytes: 134217728"));
        let mut digest = plan.digest();
        plan.managed_active_process_limit = 3;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_cpu_rate_percent = 94;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_process_memory_bytes += 1;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_job_memory_bytes += 1;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_output_hash_limit += 1;
        assert_ne!(plan.digest(), digest);
    }

    #[tokio::test]
    async fn declining_runtime_setup_creates_no_runtime_bundle() {
        let directory = tempfile::tempdir().expect("runtime root");
        let root = directory.path().join("runtimes");
        let offer = local_runtime_offers()[0];
        let expected = local_runtime_bundle_path(&root, &offer).expect("bundle path");
        let mut reader = BufReader::new(&b"\n"[..]);
        let (mut writer, mut output_reader) = tokio::io::duplex(16 * 1024);

        prompt_local_runtime_install(&mut reader, &mut writer, true, &root)
            .await
            .expect("declined runtime setup");
        drop(writer);
        let mut output = Vec::new();
        output_reader
            .read_to_end(&mut output)
            .await
            .expect("setup output");

        assert!(!expected.exists());
        assert!(String::from_utf8(output)
            .expect("utf8 setup output")
            .contains("was not downloaded"));
    }

    #[tokio::test]
    async fn declining_local_runtime_activation_mints_no_ticket_and_keeps_cli_usable() {
        let (issuer, verifier, generation) =
            argentum_security::host_authority_channel().expect("authority channel");
        let broker = Arc::new(
            argentum_platform::PlatformAuthorityBroker::bootstrap(generation.clone())
                .await
                .expect("platform authority"),
        );
        let config = argentum_providers::LocalRuntimeConfig::new(
            PathBuf::from(r"C:\argentum-test\bundle.zip"),
            PathBuf::from(r"C:\argentum-test\models"),
        )
        .expect("test runtime config");
        let (module, control) = argentum_providers::local_runtime_test_fixture(config, broker);
        let profile = argentum_domain::ProviderProfile {
            id: "llama-cpp".into(),
            label: "llama.cpp".into(),
            kind: argentum_domain::ProviderKind::LocalLlamaCpp,
            endpoint: "local://llama-cpp/".into(),
            model: "model.gguf".into(),
            selected: true,
        };
        let ticket_calls = Arc::new(AtomicUsize::new(0));
        let ticket_probe = Arc::clone(&ticket_calls);
        let mut reader = BufReader::new(&b"no\n"[..]);
        let (mut writer, mut output_reader) = tokio::io::duplex(16 * 1024);
        let prepare_module = Arc::clone(&module);
        let activate_module = Arc::clone(&module);

        let activated = run_local_runtime_terminal_activation(
            "model.gguf",
            &mut reader,
            &mut writer,
            move || async move { prepare_module.prepare(&profile).await },
            move |plan| {
                ticket_probe.fetch_add(1, Ordering::SeqCst);
                let action = argentum_security::LocalAuthorityAction::ActivateLocalRuntime {
                    plan: plan.digest(),
                };
                let ticket = issuer
                    .issue(action.clone(), generation.clone())
                    .map_err(|_| argentum_providers::ProviderError::LocalRuntimeApprovalDenied)?;
                let proof = verifier
                    .consume(ticket, &action, generation)
                    .map_err(|_| argentum_providers::ProviderError::LocalRuntimeApprovalDenied)?;
                LocalRuntimeLaunchApproval::bind(plan, proof)
            },
            move |prepared, approval| async move {
                activate_module
                    .activate(
                        prepared,
                        Some(approval),
                        tokio_util::sync::CancellationToken::new(),
                    )
                    .await
            },
        )
        .await
        .expect("declined launch ceremony");
        drop(writer);
        let mut output = Vec::new();
        output_reader
            .read_to_end(&mut output)
            .await
            .expect("ceremony output");

        assert!(!activated);
        assert_eq!(ticket_calls.load(Ordering::SeqCst), 0);
        assert_eq!(control.launcher_call_count(), 0);
        assert_eq!(control.listener_call_count(), 0);
        assert_eq!(control.network_call_count(), 0);
        assert_eq!(control.provider_call_count(), 0);
        assert!(String::from_utf8(output)
            .expect("utf8 ceremony output")
            .contains("was not started"));

        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = start_active_host(HostConfig::in_memory(workspace.path()))
            .await
            .expect("ordinary host");
        host.client()
            .dispatch(AppCommand::NewSession)
            .await
            .expect("ordinary CLI flow after decline");
        host.shutdown().await.expect("host shutdown");
    }

    #[test]
    fn serve_protocol_has_no_local_runtime_start_option() {
        assert!(parse_args(["serve", "--start-local-runtime"]).is_err());
        assert!(parse_args(["serve", "--json"]).is_err());
    }

    #[tokio::test]
    async fn active_cli_host_uses_composed_runtime_lifecycle() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = start_active_host(HostConfig::in_memory(workspace.path()))
            .await
            .expect("active host");

        let lifecycle = host
            .harness_lifecycle_snapshot()
            .await
            .expect("active lifecycle");
        assert_eq!(lifecycle.generation(), 1);
        assert_eq!(lifecycle.module_ids()[0], "first-party.review-diff");
        host.shutdown().await.expect("active host shutdown");
    }

    #[tokio::test]
    async fn one_shot_active_host_unmounts_after_operation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let observed_host = Arc::new(Mutex::new(None));
        let capture = Arc::clone(&observed_host);

        with_one_shot_host(HostConfig::in_memory(workspace.path()), move |host| {
            let capture = Arc::clone(&capture);
            Box::pin(async move {
                let lifecycle = host
                    .harness_lifecycle_snapshot()
                    .await
                    .expect("mounted lifecycle");
                assert_eq!(lifecycle.module_ids()[0], "first-party.review-diff");
                *capture.lock().expect("observed host lock") = Some(host.lifecycle_observer());
                Ok::<(), Box<dyn std::error::Error>>(())
            })
        })
        .await
        .expect("one-shot host operation");

        let observer = observed_host
            .lock()
            .expect("observed host lock")
            .clone()
            .expect("observed host");
        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("unmounted lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn failed_setup_flow_unmounts_its_owned_host() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = start_active_host(HostConfig::in_memory(workspace.path()))
            .await
            .expect("active host");
        let observer = host.lifecycle_observer();
        let mut reader = BufReader::new(&b""[..]);
        let mut writer = tokio::io::sink();
        let options = CommonOptions {
            workspace: workspace.path().to_path_buf(),
            ..CommonOptions::default()
        };

        let result =
            run_setup_flow(host, options, &mut reader, &mut writer, false, false, false).await;

        assert!(result.is_err());
        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("unmounted lifecycle")
            .module_ids()
            .is_empty());
    }

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
                goal_id: _,
                skill,
                ..
            } => {
                assert_eq!(options.workspace, PathBuf::from("workspace"));
                assert_eq!(options.database, Some(PathBuf::from("state.db")));
                assert_eq!(prompt, "inspect");
                assert_eq!(skill, None);
                assert!(json);
            }
            argument => panic!("unexpected arguments: {argument:?}"),
        }
    }

    #[test]
    fn parses_explicit_skill_run_and_inspection_commands() {
        let run = parse_args([
            "run",
            "--prompt",
            "inspect",
            "--skill",
            "rust-fix-and-verify",
        ])
        .expect("skill run arguments");
        assert!(matches!(
            run,
            Arguments::Run {
                skill: Some(name),
                ..
            } if name == "rust-fix-and-verify"
        ));

        let inspect = parse_args(["skills", "inspect", "rust-fix-and-verify", "--json"])
            .expect("skill inspect arguments");
        assert!(matches!(
            inspect,
            Arguments::Skills {
                action: SkillAction::Inspect { name },
                json: true,
                ..
            } if name == "rust-fix-and-verify"
        ));
    }

    #[test]
    fn parses_final_result_run_and_read_only_lookup() {
        let run = parse_args([
            "run",
            "--final-result",
            "--prompt",
            "read and test",
            "--workspace",
            "workspace",
        ])
        .expect("final result run arguments");
        assert!(matches!(
            run,
            Arguments::Run {
                final_result: true,
                json: false,
                prompt,
                ..
            } if prompt == "read and test"
        ));

        let run_id = "00000000-0000-0000-0000-0000000000f8";
        assert!(matches!(
            parse_args(["result", run_id]).expect("result lookup arguments"),
            Arguments::Result { run_id: parsed, .. } if parsed.to_string() == run_id
        ));
        assert!(parse_args(["run", "--final-result", "--json", "--prompt", "task"]).is_err());
        let schema_run = parse_args([
            "run",
            "--final-result",
            "--answer-schema",
            "contracts/answer.schema.json",
            "--answer-schema-sha256",
            &"ab".repeat(32),
            "--prompt",
            "task",
        ])
        .expect("schema-contracted final result arguments");
        assert!(matches!(
            schema_run,
            Arguments::Run {
                final_result: true,
                answer_schema: Some(path),
                answer_schema_sha256: Some(argentum_domain::Sha256Digest(digest)),
                ..
            } if path == "contracts/answer.schema.json" && digest == "ab".repeat(32)
        ));
        assert!(parse_args(["run", "--answer-schema", "answer.json", "--prompt", "task"]).is_err());
        assert!(parse_args([
            "run",
            "--final-result",
            "--answer-schema-sha256",
            &"ab".repeat(32),
            "--prompt",
            "task"
        ])
        .is_err());
        assert!(parse_args(["result", run_id, "--json"]).is_err());
    }

    #[test]
    fn parses_interactive_launch_and_non_tty_mode() {
        let empty = parse_args(std::iter::empty::<&str>()).expect("default launch");
        assert!(!matches!(empty, Arguments::Help));

        let chat = parse_args(["chat"]).expect("chat launch");
        assert!(!matches!(chat, Arguments::Help));

        assert!(parse_args(["--non-interactive"]).is_ok());
        assert_eq!(
            version_line(),
            concat!("argentum ", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn parses_setup_with_non_interactive_and_json_options() {
        assert!(matches!(
            parse_args(["setup", "--non-interactive", "--json"]).expect("setup arguments"),
            Arguments::Setup {
                options: CommonOptions {
                    non_interactive: true,
                    ..
                },
                json: true,
            }
        ));
        assert!(parse_args(["setup", "--prompt", "unexpected"]).is_err());
    }

    #[tokio::test]
    async fn setup_input_is_bounded_and_preserves_the_following_line() {
        let mut input = vec![b'a'; MAX_ONBOARDING_INPUT_BYTES + 1];
        input.push(b'\n');
        input.extend_from_slice(b"next\n");
        let mut reader = BufReader::new(input.as_slice());
        let mut writer = tokio::io::sink();
        let error = read_setup_line(&mut reader, &mut writer, "")
            .await
            .expect_err("oversized setup input");
        assert!(error.to_string().contains("size limit"));

        let mut reader = BufReader::new(&b"first\nsecond\n"[..]);
        let first = read_setup_line(&mut reader, &mut writer, "")
            .await
            .expect("first bounded line");
        let second = read_setup_line(&mut reader, &mut writer, "")
            .await
            .expect("second bounded line");
        assert_eq!(first, "first");
        assert_eq!(second, "second");
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
        assert!(matches!(
            parse_args(["sessions", "--query", "100%", "--archived", "--limit", "3"])
                .expect("session list query"),
            Arguments::Sessions {
                query,
                filter: SessionArchiveFilter::Archived,
                limit: 3,
                ..
            } if query == "100%"
        ));
        assert!(matches!(
            parse_args(["session", "title", session_id, "Unicode title"])
                .expect("session title"),
            Arguments::SessionOrganize {
                action: SessionAction::Title { session_id: parsed, title },
                ..
            } if parsed.to_string() == session_id && title == "Unicode title"
        ));
        assert!(matches!(
            parse_args(["session", "unarchive", session_id]).expect("session unarchive"),
            Arguments::SessionOrganize {
                action: SessionAction::Archive { session_id: parsed, archived: false },
                ..
            } if parsed.to_string() == session_id
        ));
        assert!(matches!(
            parse_args(["session", "fork", session_id, "--title", "Focused branch", "--json"])
                .expect("session fork"),
            Arguments::SessionFork {
                source_session_id: parsed,
                title,
                json: true,
                ..
            } if parsed.to_string() == session_id && title == "Focused branch"
        ));
        assert!(parse_args(["session", "fork", session_id]).is_err());
        assert!(parse_args(["version", "extra"]).is_err());
        assert!(parse_args(["session", "select", "invalid"]).is_err());
        assert_eq!(
            version_line(),
            concat!("argentum ", env!("CARGO_PKG_VERSION"))
        );
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
        let goal_id = "00000000-0000-0000-0000-0000000000b7";
        assert!(matches!(
            parse_args([
                "goal",
                "criteria",
                "--goal-id",
                goal_id,
                "--runners",
                "cargo-fmt-check,cargo-check-locked",
                "--owner-attestation-required",
                "--json",
            ])
            .expect("goal criteria"),
            Arguments::Goal {
                action: GoalAction::Criteria {
                    goal_id: parsed,
                    runners,
                    owner_attestation_required: true,
                },
                json: true,
                ..
            } if parsed.to_string() == goal_id
                && runners == [
                    argentum_domain::VerificationRunnerId::CargoFmtCheck,
                    argentum_domain::VerificationRunnerId::CargoCheckLocked,
                ]
        ));
        assert!(matches!(
            parse_args([
                "goal",
                "attest",
                "--goal-id",
                goal_id,
                "--summary",
                "Owner reviewed the subjective scope",
            ])
            .expect("goal attestation"),
            Arguments::Goal {
                action: GoalAction::Attest {
                    goal_id: parsed,
                    summary,
                },
                ..
            } if parsed.to_string() == goal_id
                && summary == "Owner reviewed the subjective scope"
        ));
        assert!(matches!(
            parse_args([
                "goal",
                "supervise",
                "--goal-id",
                goal_id,
                "--iterations",
                "3",
                "--elapsed-seconds",
                "600",
                "--json",
            ])
            .expect("goal supervision"),
            Arguments::Goal {
                action: GoalAction::Supervise {
                    goal_id: parsed,
                    iteration_limit: 3,
                    elapsed_seconds: 600,
                },
                json: true,
                ..
            } if parsed.to_string() == goal_id
        ));
        assert!(parse_args(["goal", "set"]).is_err());
        assert!(parse_args(["goal", "set", "Objective", "--token-budget", "0"]).is_ok());
        assert!(parse_args(["goal", "pause", "--token-budget", "10"]).is_err());
        assert!(parse_args(["goal", "criteria", "--goal-id", goal_id, "--runners", ""]).is_err());
        assert!(parse_args([
            "goal",
            "criteria",
            "--goal-id",
            goal_id,
            "--runners",
            "arbitrary-shell"
        ])
        .is_err());
        assert!(parse_args([
            "goal",
            "supervise",
            "--goal-id",
            goal_id,
            "--iterations",
            "3"
        ])
        .is_err());
    }

    #[test]
    fn phase4_parses_tool_permission_and_verification_commands() {
        assert!(matches!(
            parse_args(["tool", "list", "--json"]).expect("tool list"),
            Arguments::Tools { json: true, .. }
        ));
        assert!(matches!(
            parse_args(["permissions", "list"]).expect("permission list"),
            Arguments::Permissions {
                action: PermissionAction::List,
                ..
            }
        ));
        assert!(matches!(
            parse_args(["permissions", "select", "full-access"])
                .expect("permission select"),
            Arguments::Permissions {
                action: PermissionAction::Select { profile_id },
                ..
            } if profile_id == "full-access"
        ));
        let grant_id = "00000000-0000-0000-0000-0000000000f1";
        assert!(matches!(
            parse_args(["permissions", "revoke", grant_id]).expect("permission revoke"),
            Arguments::Permissions {
                action: PermissionAction::Revoke { grant_id: parsed },
                ..
            } if parsed.to_string() == grant_id
        ));
        assert!(matches!(
            parse_args(["verification", "list", "--json"]).expect("verification list"),
            Arguments::Verification {
                action: VerificationAction::List,
                json: true,
                ..
            }
        ));
        let run_id = "00000000-0000-0000-0000-0000000000f2";
        assert!(matches!(
            parse_args(["verification", "status", run_id]).expect("verification status"),
            Arguments::Verification {
                action: VerificationAction::Status { run_id: Some(parsed) },
                ..
            } if parsed.to_string() == run_id
        ));
        assert!(parse_args(["tool", "run"]).is_err());
        assert!(parse_args(["permissions", "select"]).is_err());
        assert!(parse_args(["permissions", "revoke", "not-an-id"]).is_err());
        assert!(parse_args(["verification", "status", "not-a-run"]).is_err());
    }

    #[test]
    fn parses_harness_inspection_presentation_and_execution_commands() {
        for (verb, expected) in [
            ("install", argentum_domain::ModuleAction::Install),
            ("enable", argentum_domain::ModuleAction::Enable),
            ("disable", argentum_domain::ModuleAction::Disable),
            ("remove", argentum_domain::ModuleAction::Remove),
        ] {
            assert!(
                matches!(parse_args(["harness", "module", "first-party.activity", verb, "--json"]).unwrap(), Arguments::Harness { action: HarnessAction::SetModule { module_id, action }, json: true, .. } if module_id == "first-party.activity" && action == expected)
            );
        }
        assert!(parse_args(["harness", "module", "first-party.activity"]).is_err());
        assert!(parse_args(["harness", "module", "first-party.activity", "show"]).is_err());
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
        assert!(matches!(
            parse_args(["harness", "trajectory", "--json"]).expect("active session trajectory"),
            Arguments::Harness {
                action: HarnessAction::Trajectory { session_id: None },
                json: true,
                ..
            }
        ));
        let trajectory_session = "00000000-0000-0000-0000-0000000000a1";
        assert!(matches!(
            parse_args(["harness", "trajectory", trajectory_session])
                .expect("explicit session trajectory"),
            Arguments::Harness {
                action: HarnessAction::Trajectory { session_id: Some(session_id) },
                ..
            } if session_id.to_string() == trajectory_session
        ));
        assert!(parse_args(["harness", "surface", "terminal"]).is_err());
        assert!(parse_args(["harness", "surface", "unknown", "show"]).is_err());
        assert!(parse_args(["harness", "profile"]).is_err());
        assert!(parse_args(["harness", "execution"]).is_err());
        assert!(parse_args(["harness", "capability", "tool.read-text", "show"]).is_err());
        assert!(parse_args(["harness", "trajectory", "not-a-session"]).is_err());
    }

    #[test]
    fn parses_explicit_change_inspection_restore_and_recovery_commands() {
        let run_id = "00000000-0000-0000-0000-0000000000c1";
        let file_change_id = "00000000-0000-0000-0000-0000000000c2";
        let operation_id = "00000000-0000-0000-0000-0000000000c3";

        assert!(matches!(
            parse_args(["changes", "list", run_id, "--json"]).expect("changes list"),
            Arguments::Changes {
                action: ChangeAction::List { run_id: parsed },
                json: true,
                ..
            } if parsed.to_string() == run_id
        ));
        assert!(matches!(
            parse_args(["changes", "diff", file_change_id]).expect("changes diff"),
            Arguments::Changes {
                action: ChangeAction::Diff { file_change_id: parsed },
                ..
            } if parsed.to_string() == file_change_id
        ));
        assert!(matches!(
            parse_args(["changes", "restore", file_change_id]).expect("changes restore"),
            Arguments::Changes {
                action: ChangeAction::Restore { file_change_id: parsed },
                ..
            } if parsed.to_string() == file_change_id
        ));
        assert!(matches!(
            parse_args(["changes", "recovery"]).expect("recovery status"),
            Arguments::Changes {
                action: ChangeAction::Recovery,
                ..
            }
        ));
        assert!(matches!(
            parse_args(["changes", "recovery", "retry", operation_id])
                .expect("recovery retry"),
            Arguments::Changes {
                action: ChangeAction::RetryRecovery { operation_id: parsed },
                ..
            } if parsed.to_string() == operation_id
        ));
        assert!(parse_args(["changes", "restore", "not-an-id"]).is_err());
        assert!(parse_args(["changes", "restore", file_change_id, "--yes"]).is_err());
    }

    #[test]
    fn parses_local_owner_recovery_commands_without_host_options() {
        let run_id = "00000000-0000-0000-0000-0000000000d1";
        let revision = "00000000-0000-0000-0000-0000000000d2";
        let database = r"C:\Argentum\state.db";
        assert!(matches!(
            parse_args(["recovery", "inspect", "--database", database, "--json"])
                .expect("recovery inspection"),
            Arguments::Recovery {
                database: parsed,
                action: RecoveryAction::Inspect,
                json: true,
            } if parsed == Path::new(database)
        ));
        assert!(matches!(
            parse_args(["recovery", "reconcile", "--database", database])
                .expect("recovery reconciliation"),
            Arguments::Recovery {
                action: RecoveryAction::Reconcile,
                json: false,
                ..
            }
        ));
        assert!(matches!(
            parse_args([
                "recovery",
                "review",
                "--run-id",
                run_id,
                "--revision",
                revision,
                "--database",
                database,
            ])
            .expect("recovery review"),
            Arguments::Recovery {
                action: RecoveryAction::Review {
                    run_id: parsed_run,
                    revision: parsed_revision,
                },
                ..
            } if parsed_run.to_string() == run_id && parsed_revision.to_string() == revision
        ));
        assert!(parse_args(["recovery", "review", "--run-id", run_id]).is_err());
        assert!(parse_args(["recovery", "inspect", "--workspace", "."]).is_err());
        assert!(parse_args(["recovery", "reconcile", "--run-id", run_id]).is_err());
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
            parse_args(["provider", "add", "deepseek", "--json"]).unwrap(),
            Arguments::ProviderAdd { preset_id, json: true, .. } if preset_id == "deepseek"
        ));
        assert!(parse_args(["provider", "add"]).is_err());
        assert!(parse_args(["provider", "add", "unknown"]).is_err());
        assert!(parse_args([
            "provider",
            "add",
            "deepseek",
            "--endpoint",
            "https://example.com/v1/"
        ])
        .is_err());
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
                "openai-compatible",
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
                    && profile.kind == argentum_domain::ProviderKind::OpenAiCompatible
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

    #[tokio::test]
    async fn isolated_cli_uses_only_explicit_paths_without_credentials_or_local_runtime() {
        let workspace = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let database = data.path().join("isolated.db");
        let arguments = vec![
            "status".to_owned(),
            "--isolated".into(),
            "--workspace".into(),
            workspace.path().display().to_string(),
            "--database".into(),
            database.display().to_string(),
        ];
        let Arguments::Status { options, .. } = parse_args(arguments).unwrap() else {
            panic!("status arguments");
        };
        let config = options.host_config().unwrap();
        assert_eq!(config.database.as_ref(), Some(&database));
        assert!(config.local_runtime.is_none());
        assert!(config.android_inference.is_none());
        let mut host = start_active_host(config).await.unwrap();
        for id in ["openai", "minimax", "deepseek"] {
            assert!(!host.provider_credential_configured(id));
        }
        host.shutdown().await.unwrap();
        assert!(parse_args(["status", "--isolated"]).is_err());
        assert!(parse_args(["workspace", "set", ".", "--isolated"]).is_err());
        assert!(parse_args(["setup", "--isolated"]).is_err());
        assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
    }

    #[test]
    fn model_picker_accepts_bounded_indices_exact_ids_and_optional_manual_entry() {
        let models = vec![
            argentum_domain::ProviderModel {
                id: "qwen-small".into(),
                label: "Qwen small".into(),
                context_window_tokens: None,
            },
            argentum_domain::ProviderModel {
                id: "remote-model".into(),
                label: "Remote model".into(),
                context_window_tokens: Some(32_768),
            },
        ];

        assert_eq!(
            model_picker::select_model(&models, "2", false).expect("indexed model"),
            "remote-model"
        );
        assert_eq!(
            model_picker::select_model(&models, "qwen-small", false).expect("exact model ID"),
            "qwen-small"
        );
        assert_eq!(
            model_picker::select_model(&models, "custom-model", true).expect("manual model"),
            "custom-model"
        );
        assert!(model_picker::select_model(&models, "custom-model", false).is_err());
        assert_eq!(
            model_picker::filter_models(&models, "QWEN")
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["qwen-small"]
        );
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
    fn chatgpt_owner_commands_do_not_accept_tokens_or_custom_endpoints() {
        for action in ["login", "login-new", "logout", "accounts"] {
            assert!(matches!(
                parse_args(["provider", action, "chatgpt"]).unwrap(),
                Arguments::ChatGptAuth { .. }
            ));
            assert!(parse_args([
                "provider",
                action,
                "chatgpt",
                "--endpoint",
                "https://evil.invalid/"
            ])
            .is_err());
            assert!(parse_args(["provider", action, "deepseek"]).is_err());
        }
        assert!(matches!(
            parse_args(["provider", "account", "chatgpt", "synthetic_id"]).unwrap(),
            Arguments::ChatGptAuth {
                account: Some(_),
                ..
            }
        ));
        assert!(parse_args(["provider", "account", "chatgpt"]).is_err());
        assert!(parse_args(["provider", "login", "chatgpt", "SYNTHETIC_TOKEN"]).is_err());
        assert!(parse_args([
            "provider",
            "credential",
            "set",
            "chatgpt",
            "SYNTHETIC_TOKEN"
        ])
        .is_err());
    }

    #[test]
    fn formats_selected_provider_profile_for_human_output() {
        let profiles = vec![argentum_domain::ProviderProfile {
            id: "local-secondary".into(),
            label: "Secondary local".into(),
            kind: argentum_domain::ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:5678/v1/".into(),
            model: "secondary-model".into(),
            selected: true,
        }];

        assert_eq!(
            format_provider_profiles_human(&profiles),
            "* local-secondary  Secondary local  openai-compatible  secondary-model  http://127.0.0.1:5678/v1/\n"
        );
    }

    #[test]
    fn formats_truthful_provider_connection_state() {
        let connected = argentum_domain::ProviderStatus {
            profile_id: "openai-compatible".into(),
            kind: argentum_domain::ProviderKind::OpenAiCompatible,
            label: "OpenAI-compatible".into(),
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
