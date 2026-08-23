use std::collections::HashSet;
use std::sync::Arc;

use argentum_domain::AppCommand;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinSet;

use crate::protocol::{ClientPayload, RequestEnvelope, ResponseEnvelope, ServerPayload};
use crate::CommandHost;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_IN_FLIGHT_COMMANDS: usize = 32;
const RESPONSE_QUEUE_CAPACITY: usize = 256;

pub async fn serve_jsonl<R, W>(host: CommandHost, reader: R, writer: W) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    serve_jsonl_with_limit(host, reader, writer, MAX_IN_FLIGHT_COMMANDS).await
}

async fn serve_jsonl_with_limit<R, W>(
    host: CommandHost,
    mut reader: R,
    mut writer: W,
    max_in_flight: usize,
) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (responses, mut response_receiver) =
        mpsc::channel::<OutboundResponse>(RESPONSE_QUEUE_CAPACITY);
    let workspace = display_workspace(host.workspace_root());
    responses
        .send(OutboundResponse {
            request_id: None,
            payload: ServerPayload::Ready { workspace },
        })
        .await
        .map_err(channel_closed)?;

    let writer_task = tokio::spawn(async move {
        let mut sequence = 0_u64;
        while let Some(response) = response_receiver.recv().await {
            sequence = sequence.saturating_add(1);
            write_response(
                &mut writer,
                &ResponseEnvelope::new(sequence, response.request_id, response.payload),
            )
            .await?;
        }
        writer.flush().await
    });

    let mut events = host.subscribe();
    let event_sender = responses.clone();
    let (event_shutdown, mut event_shutdown_receiver) = oneshot::channel::<()>();
    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                event = events.recv() => {
                    let Some(payload) = event_payload(event) else {
                        break;
                    };
                    if !send_event_payload(&event_sender, payload).await {
                        break;
                    }
                }
                _ = &mut event_shutdown_receiver => {
                    loop {
                        let payload = match events.try_recv() {
                            Ok(event) => ServerPayload::Event { event },
                            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                                ServerPayload::error(
                                    "event_lagged",
                                    format!("event subscriber lagged by {skipped} events"),
                                    true,
                                )
                            }
                            Err(
                                tokio::sync::broadcast::error::TryRecvError::Empty
                                | tokio::sync::broadcast::error::TryRecvError::Closed,
                            ) => break,
                        };
                        if !send_event_payload(&event_sender, payload).await {
                            break;
                        }
                    }
                    break;
                }
            }
        }
    });

    if let Err(error_value) = host.publish_initial_state() {
        responses
            .send(OutboundResponse {
                request_id: None,
                payload: ServerPayload::error(
                    "initialization_failed",
                    error_value.to_string(),
                    true,
                ),
            })
            .await
            .map_err(channel_closed)?;
    }

    let permits = Arc::new(Semaphore::new(max_in_flight.max(1)));
    let active_request_ids = Arc::new(tokio::sync::Mutex::new(HashSet::<String>::new()));
    let mut commands = JoinSet::new();
    let mut buffer = Vec::with_capacity(4096);

    loop {
        buffer.clear();
        let bytes_read = (&mut reader)
            .take((MAX_REQUEST_BYTES + 2) as u64)
            .read_until(b'\n', &mut buffer)
            .await?;
        if bytes_read == 0 {
            break;
        }
        if buffer.len() > MAX_REQUEST_BYTES {
            responses
                .send(OutboundResponse {
                    request_id: None,
                    payload: ServerPayload::error(
                        "request_too_large",
                        format!("request exceeds the {MAX_REQUEST_BYTES} byte limit"),
                        true,
                    ),
                })
                .await
                .map_err(channel_closed)?;
            if !buffer.ends_with(b"\n") {
                discard_line_remainder(&mut reader).await?;
            }
            continue;
        }

        while matches!(buffer.last(), Some(b'\n' | b'\r')) {
            buffer.pop();
        }
        if buffer.is_empty() {
            continue;
        }
        let request = match serde_json::from_slice::<RequestEnvelope>(&buffer) {
            Ok(request) => request,
            Err(error_value) => {
                responses
                    .send(OutboundResponse {
                        request_id: None,
                        payload: ServerPayload::error(
                            "invalid_json",
                            format!("invalid request: {error_value}"),
                            true,
                        ),
                    })
                    .await
                    .map_err(channel_closed)?;
                continue;
            }
        };
        let request_id = request.request_id.clone();
        if let Err(error_value) = request.validate() {
            responses
                .send(OutboundResponse {
                    request_id: Some(request_id),
                    payload: ServerPayload::error("invalid_request", error_value.to_string(), true),
                })
                .await
                .map_err(channel_closed)?;
            continue;
        }

        match request.payload {
            ClientPayload::Ping => {
                responses
                    .send(OutboundResponse {
                        request_id: Some(request_id),
                        payload: ServerPayload::Pong,
                    })
                    .await
                    .map_err(channel_closed)?;
            }
            ClientPayload::Command { command } => {
                if !jsonl_command_is_permitted(&command) {
                    responses
                        .send(OutboundResponse {
                            request_id: Some(request_id),
                            payload: ServerPayload::error(
                                "command_not_permitted",
                                "command is not permitted on the untrusted JSONL transport",
                                false,
                            ),
                        })
                        .await
                        .map_err(channel_closed)?;
                    continue;
                }
                {
                    let mut active = active_request_ids.lock().await;
                    if !active.insert(request_id.clone()) {
                        drop(active);
                        responses
                            .send(OutboundResponse {
                                request_id: Some(request_id),
                                payload: ServerPayload::error(
                                    "duplicate_request_id",
                                    "request_id is already active",
                                    true,
                                ),
                            })
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }
                }
                let client = host.client();
                let permit = if is_long_running(&command) {
                    match permits.clone().try_acquire_owned() {
                        Ok(permit) => Some(permit),
                        Err(_) => {
                            responses
                                .send(OutboundResponse {
                                    request_id: Some(request_id.clone()),
                                    payload: ServerPayload::error(
                                        "server_busy",
                                        "the command concurrency limit is active; retry this request",
                                        true,
                                    ),
                                })
                                .await
                                .map_err(channel_closed)?;
                            active_request_ids.lock().await.remove(&request_id);
                            continue;
                        }
                    }
                } else {
                    None
                };
                responses
                    .send(OutboundResponse {
                        request_id: Some(request_id.clone()),
                        payload: ServerPayload::CommandAccepted,
                    })
                    .await
                    .map_err(channel_closed)?;
                let Some(permit) = permit else {
                    let payload = command_result(client.dispatch(command).await);
                    responses
                        .send(OutboundResponse {
                            request_id: Some(request_id.clone()),
                            payload,
                        })
                        .await
                        .map_err(channel_closed)?;
                    active_request_ids.lock().await.remove(&request_id);
                    continue;
                };
                let sender = responses.clone();
                let active_request_ids = active_request_ids.clone();
                commands.spawn(async move {
                    let _permit = permit;
                    let payload = command_result(client.dispatch(command).await);
                    let _ = sender
                        .send(OutboundResponse {
                            request_id: Some(request_id.clone()),
                            payload,
                        })
                        .await;
                    active_request_ids.lock().await.remove(&request_id);
                });
            }
        }
    }

    commands.join_all().await;
    let _ = event_shutdown.send(());
    let _ = event_task.await;
    drop(responses);
    writer_task.await.map_err(std::io::Error::other)??;
    Ok(())
}

fn jsonl_command_is_permitted(command: &AppCommand) -> bool {
    match command {
        AppCommand::NewSession
        | AppCommand::SelectSession { .. }
        | AppCommand::SetGoal { .. }
        | AppCommand::PauseGoal
        | AppCommand::ResumeGoal
        | AppCommand::ClearGoal
        | AppCommand::ProbeProvider { .. }
        | AppCommand::ListProviderProfiles
        | AppCommand::ListProviderModels { .. }
        | AppCommand::ListHarnessState
        | AppCommand::SetSurfaceVisibility { .. }
        | AppCommand::LoadTrajectory { .. }
        | AppCommand::SubmitTask { .. }
        | AppCommand::CancelRun { .. }
        | AppCommand::ToggleSurface { .. }
        | AppCommand::SetLayout { .. }
        | AppCommand::ResetLayout => true,
        AppCommand::SaveProviderProfile { .. }
        | AppCommand::SelectProviderProfile { .. }
        | AppCommand::SelectProviderModel { .. }
        | AppCommand::SelectHarnessProfile { .. }
        | AppCommand::SelectExecutionProfile { .. }
        | AppCommand::SetHarnessCapabilityEnabled { .. }
        | AppCommand::RequestTool { .. }
        | AppCommand::ApproveTool { .. }
        | AppCommand::RejectTool { .. } => false,
    }
}

fn is_long_running(command: &AppCommand) -> bool {
    matches!(
        command,
        AppCommand::SubmitTask { .. }
            | AppCommand::ProbeProvider { .. }
            | AppCommand::ListProviderModels { .. }
            | AppCommand::RequestTool { .. }
            | AppCommand::ApproveTool { .. }
    )
}

fn command_result(result: Result<(), crate::HostError>) -> ServerPayload {
    match result {
        Ok(()) => ServerPayload::CommandCompleted,
        Err(error_value) => {
            ServerPayload::command_failed("command_failed", error_value.to_string(), true)
        }
    }
}

#[derive(Debug)]
struct OutboundResponse {
    request_id: Option<String>,
    payload: ServerPayload,
}

fn event_payload(
    event: Result<argentum_domain::AppEvent, tokio::sync::broadcast::error::RecvError>,
) -> Option<ServerPayload> {
    match event {
        Ok(event) => Some(ServerPayload::Event { event }),
        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
            Some(ServerPayload::error(
                "event_lagged",
                format!("event subscriber lagged by {skipped} events"),
                true,
            ))
        }
        Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
    }
}

async fn send_event_payload(
    sender: &mpsc::Sender<OutboundResponse>,
    payload: ServerPayload,
) -> bool {
    sender
        .send(OutboundResponse {
            request_id: None,
            payload,
        })
        .await
        .is_ok()
}

async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &ResponseEnvelope,
) -> std::io::Result<()> {
    let mut encoded = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    writer.flush().await
}

async fn discard_line_remainder<R: AsyncBufRead + Unpin>(reader: &mut R) -> std::io::Result<()> {
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(());
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        let found_newline = available.get(consumed.saturating_sub(1)) == Some(&b'\n');
        reader.consume(consumed);
        if found_newline {
            return Ok(());
        }
    }
}

fn channel_closed(error: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, error.to_string())
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use argentum_domain::{AppCommand, AppEvent, ApprovalScope, ToolInput, ToolRequest};
    use argentum_providers::{ModelProvider, ModelRequest, ProviderEvent, ProviderRegistry};
    use argentum_runtime::RuntimeService;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use argentum_store::Store;
    use argentum_tools::ToolRegistry;
    use argentum_workspaces::WorkspaceManager;
    use async_trait::async_trait;
    use tokio::io::{AsyncReadExt, BufReader};
    use tokio::sync::mpsc;

    use super::*;
    use crate::protocol::PROTOCOL_VERSION;
    use crate::HostConfig;

    #[tokio::test]
    async fn protocol_dispatch_matches_in_process_runtime_events() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");

        let direct = host.client();
        let mut direct_events = direct.subscribe();
        direct
            .dispatch(AppCommand::NewSession)
            .await
            .expect("direct command");
        let direct_kind = direct_events.recv().await.expect("direct event").kind();

        let input = format!(
            "{}\n",
            serde_json::to_string(&RequestEnvelope::command("wire-1", AppCommand::NewSession))
                .expect("request")
        );
        let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);
        serve_jsonl(host, BufReader::new(input.as_bytes()), output_writer)
            .await
            .expect("server");
        let mut output = String::new();
        output_reader
            .read_to_string(&mut output)
            .await
            .expect("output");
        let responses = parse_responses(&output);
        let protocol_kind = responses
            .iter()
            .find_map(|response| match &response.payload {
                ServerPayload::Event { event } if event.kind() == "session_created" => {
                    Some(event.kind())
                }
                _ => None,
            })
            .expect("session event");

        assert_eq!(direct_kind, protocol_kind);
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("wire-1")
                && matches!(response.payload, ServerPayload::CommandAccepted)
        }));
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("wire-1")
                && matches!(response.payload, ServerPayload::CommandCompleted)
        }));
    }

    #[tokio::test]
    async fn selected_provider_without_credentials_has_a_failed_terminal_outcome() {
        let responses = submit_task_responses(ProviderRegistry::default()).await;

        assert_failed_command_outcome(&responses, "credentials are required");
    }

    #[tokio::test]
    async fn provider_stream_failure_has_a_failed_terminal_outcome() {
        let mut providers = ProviderRegistry::default();
        providers.register(FailingProvider);

        let responses = submit_task_responses(providers).await;

        assert_failed_command_outcome(&responses, "test provider failure");
    }

    #[tokio::test]
    async fn provider_probe_failure_emits_safe_events_and_failed_terminal() {
        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let workspace_manager = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace_manager.clone());
        let mut providers = ProviderRegistry::default();
        providers.register(FailingProvider);
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace_manager,
        )
        .expect("runtime");
        let responses = serve_requests(
            CommandHost::from_runtime(runtime),
            &[RequestEnvelope::command(
                "probe-failure",
                AppCommand::ProbeProvider {
                    provider_id: "failing".into(),
                },
            )],
        )
        .await;
        let rendered = format!("{responses:?}");

        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event {
                event: AppEvent::ProviderStatus(status),
            } if !status.connected && status.detail.contains("probe failed")
        )));
        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event {
                event: AppEvent::Error { message, recoverable: true },
            } if message.contains("probe failed")
        )));
        assert!(matches!(
            responses
                .iter()
                .rfind(|response| response.request_id.as_deref() == Some("probe-failure"))
                .map(|response| &response.payload),
            Some(ServerPayload::CommandFailed { .. })
        ));
        assert!(!responses.iter().any(|response| {
            response.request_id.as_deref() == Some("probe-failure")
                && matches!(response.payload, ServerPayload::CommandCompleted)
        }));
        assert!(!rendered.contains("secret response body"));
    }

    #[tokio::test]
    async fn protocol_rejects_direct_tool_commands_from_untrusted_clients() {
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace.path().join("note.txt"), "workspace note").expect("fixture");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let call_id = "00000000-0000-0000-0000-000000000041"
            .parse()
            .expect("call id");
        let request = RequestEnvelope::command(
            "read-1",
            AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id: "00000000-0000-0000-0000-000000000042"
                        .parse()
                        .expect("run id"),
                    input: ToolInput::ReadText {
                        path: "note.txt".into(),
                    },
                },
            },
        );

        let responses = serve_requests(host, &[request]).await;

        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("read-1")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. } if code == "command_not_permitted"
                )
        }));
        assert!(!responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event {
                event: AppEvent::ToolStarted(_) | AppEvent::ToolFinished(_),
            }
        )));
        assert!(!responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event {
                event: AppEvent::ApprovalRequested(_),
            }
        )));
    }

    #[tokio::test]
    async fn protocol_rejects_commands_that_change_authority_or_network_origins() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let call_id = "00000000-0000-0000-0000-000000000051"
            .parse()
            .expect("call id");
        let run_id = "00000000-0000-0000-0000-000000000053"
            .parse()
            .expect("run id");
        let approval_id = "00000000-0000-0000-0000-000000000054"
            .parse()
            .expect("approval id");
        let requests = [
            RequestEnvelope::command(
                "write",
                AppCommand::RequestTool {
                    request: ToolRequest {
                        call_id,
                        run_id,
                        input: ToolInput::WriteText {
                            path: "blocked.txt".into(),
                            content: "blocked content".into(),
                        },
                    },
                },
            ),
            RequestEnvelope::command(
                "approve",
                AppCommand::ApproveTool {
                    approval_id,
                    scope: ApprovalScope::Once,
                },
            ),
            RequestEnvelope::command(
                "provider",
                AppCommand::SaveProviderProfile {
                    profile: argentum_domain::ProviderProfile {
                        id: "attacker".into(),
                        label: "Attacker".into(),
                        kind: argentum_domain::ProviderKind::OpenAiCompatible,
                        endpoint: "http://127.0.0.1:9/v1/".into(),
                        model: "capture".into(),
                        selected: true,
                    },
                },
            ),
            RequestEnvelope::command(
                "profile",
                AppCommand::SelectExecutionProfile {
                    profile_id: "autonomous".into(),
                },
            ),
        ];
        let responses = serve_requests(host, &requests).await;

        for request_id in ["write", "approve", "provider", "profile"] {
            assert!(responses.iter().any(|response| {
                response.request_id.as_deref() == Some(request_id)
                    && matches!(
                        &response.payload,
                        ServerPayload::Error { code, .. } if code == "command_not_permitted"
                    )
            }));
        }
        assert!(!workspace.path().join("blocked.txt").exists());
    }

    #[tokio::test]
    async fn long_running_command_does_not_block_a_second_acceptance() {
        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let workspace_manager = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace_manager.clone());
        let mut providers = ProviderRegistry::default();
        providers.register(PendingProvider);
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace_manager,
        )
        .expect("runtime");
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: argentum_domain::ProviderProfile {
                    id: "pending".into(),
                    label: "Pending".into(),
                    kind: argentum_domain::ProviderKind::OpenAiCompatible,
                    endpoint: "https://pending.example.test/v1/".into(),
                    model: "test-model".into(),
                    selected: true,
                },
            })
            .await
            .expect("selected pending provider");
        let host = CommandHost::from_runtime(runtime);
        let first = RequestEnvelope::command(
            "slow",
            AppCommand::SubmitTask {
                prompt: "wait".into(),
            },
        );
        let second = RequestEnvelope::command("quick", AppCommand::NewSession);
        let input = format!(
            "{}\n{}\n",
            serde_json::to_string(&first).expect("first"),
            serde_json::to_string(&second).expect("second")
        );
        let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            serve_jsonl(host, BufReader::new(input.as_bytes()), output_writer).await
        });

        let mut output = Vec::new();
        let read_result = tokio::time::timeout(Duration::from_secs(1), async {
            let mut chunk = [0_u8; 4096];
            loop {
                let count = output_reader.read(&mut chunk).await?;
                if count == 0 {
                    return Ok::<(), std::io::Error>(());
                }
                output.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&output);
                if text.lines().any(|line| {
                    serde_json::from_str::<ResponseEnvelope>(line).is_ok_and(|response| {
                        response.request_id.as_deref() == Some("quick")
                            && matches!(
                                response.payload,
                                ServerPayload::CommandAccepted | ServerPayload::CommandCompleted
                            )
                    })
                }) {
                    return Ok(());
                }
            }
        })
        .await;

        assert!(matches!(read_result, Ok(Ok(()))), "second command stalled");
        server.abort();
    }

    #[tokio::test]
    async fn invalid_version_returns_a_correlated_error() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let mut request = RequestEnvelope::command("bad-version", AppCommand::NewSession);
        request.protocol_version = PROTOCOL_VERSION + 1;
        let input = format!("{}\n", serde_json::to_string(&request).expect("request"));
        let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);
        serve_jsonl(host, BufReader::new(input.as_bytes()), output_writer)
            .await
            .expect("server");
        let mut output = String::new();
        output_reader
            .read_to_string(&mut output)
            .await
            .expect("output");

        assert!(parse_responses(&output).iter().any(|response| {
            response.request_id.as_deref() == Some("bad-version")
                && matches!(response.payload, ServerPayload::Error { .. })
        }));
    }

    #[tokio::test]
    async fn malformed_and_complete_oversized_lines_do_not_consume_the_next_request() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let ping = RequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: "after-errors".into(),
            payload: ClientPayload::Ping,
        };
        let mut input = b"{not-json}\n".to_vec();
        input.extend(std::iter::repeat_n(b'x', MAX_REQUEST_BYTES + 8192));
        input.push(b'\n');
        input.extend(serde_json::to_vec(&ping).expect("ping"));
        input.push(b'\n');
        let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);

        serve_jsonl(host, BufReader::new(input.as_slice()), output_writer)
            .await
            .expect("server");
        let mut output = String::new();
        output_reader
            .read_to_string(&mut output)
            .await
            .expect("output");
        let responses = parse_responses(&output);

        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Error { code, .. } if code == "invalid_json"
        )));
        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Error { code, .. } if code == "request_too_large"
        )));
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("after-errors")
                && matches!(response.payload, ServerPayload::Pong)
        }));
    }

    fn parse_responses(output: &str) -> Vec<ResponseEnvelope> {
        output
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid JSONL response"))
            .collect()
    }

    async fn submit_task_responses(providers: ProviderRegistry) -> Vec<ResponseEnvelope> {
        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let workspace_manager = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace_manager.clone());
        let has_failing_provider = providers.status("failing").is_some();
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace_manager,
        )
        .expect("runtime");
        let profile = if has_failing_provider {
            argentum_domain::ProviderProfile {
                id: "failing".into(),
                label: "Failing".into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                endpoint: "https://example.test/v1/".into(),
                model: "test-model".into(),
                selected: true,
            }
        } else {
            argentum_domain::ProviderProfile {
                id: "anthropic-test".into(),
                label: "Anthropic test".into(),
                kind: argentum_domain::ProviderKind::Anthropic,
                endpoint: "https://api.anthropic.com/v1/messages".into(),
                model: "claude-test".into(),
                selected: true,
            }
        };
        runtime
            .dispatch(AppCommand::SaveProviderProfile { profile })
            .await
            .expect("selected test provider");
        let host = CommandHost::from_runtime(runtime);
        let input = format!(
            "{}\n",
            serde_json::to_string(&RequestEnvelope::command(
                "run-failure",
                AppCommand::SubmitTask {
                    prompt: "inspect".into(),
                },
            ))
            .expect("request")
        );
        let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);

        serve_jsonl(host, BufReader::new(input.as_bytes()), output_writer)
            .await
            .expect("server");
        let mut output = String::new();
        output_reader
            .read_to_string(&mut output)
            .await
            .expect("output");
        parse_responses(&output)
    }

    fn assert_failed_command_outcome(responses: &[ResponseEnvelope], expected_message: &str) {
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("run-failure")
                && matches!(response.payload, ServerPayload::CommandAccepted)
        }));
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("run-failure")
                && matches!(
                    &response.payload,
                    ServerPayload::CommandFailed { code, message, .. }
                        if code == "command_failed" && message.contains(expected_message)
                )
        }));
        assert!(!responses.iter().any(|response| {
            response.request_id.as_deref() == Some("run-failure")
                && matches!(response.payload, ServerPayload::CommandCompleted)
        }));
        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event {
                event: argentum_domain::AppEvent::RunStatusChanged {
                    lifecycle: argentum_domain::TaskLifecycle::Failed,
                    ..
                }
            }
        )));
    }

    async fn serve_requests(
        host: CommandHost,
        requests: &[RequestEnvelope],
    ) -> Vec<ResponseEnvelope> {
        let mut input = requests
            .iter()
            .map(|request| serde_json::to_string(request).expect("request"))
            .collect::<Vec<_>>()
            .join("\n");
        input.push('\n');
        let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);
        serve_jsonl(host, BufReader::new(input.as_bytes()), output_writer)
            .await
            .expect("server");
        let mut output = String::new();
        output_reader
            .read_to_string(&mut output)
            .await
            .expect("output");
        parse_responses(&output)
    }

    #[derive(Debug)]
    struct PendingProvider;

    #[async_trait]
    impl ModelProvider for PendingProvider {
        fn id(&self) -> &'static str {
            "pending"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "Pending".into(),
                endpoint: "local".into(),
                connected: true,
                detail: "test".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            _sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), argentum_providers::ProviderError> {
            std::future::pending().await
        }
    }

    #[derive(Debug)]
    struct FailingProvider;

    #[async_trait]
    impl ModelProvider for FailingProvider {
        fn id(&self) -> &'static str {
            "failing"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "Failing".into(),
                endpoint: "local".into(),
                connected: false,
                detail: "test".into(),
            }
        }

        async fn probe(
            &self,
        ) -> Result<argentum_domain::ProviderStatus, argentum_providers::ProviderError> {
            Err(argentum_providers::ProviderError::Api(
                "secret response body".into(),
            ))
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            _sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), argentum_providers::ProviderError> {
            Err(argentum_providers::ProviderError::Api(
                "test provider failure".into(),
            ))
        }
    }
}
