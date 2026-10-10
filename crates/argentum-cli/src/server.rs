use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use argentum_domain::AppCommand;
use argentum_runtime::{SubmissionTarget, TaskSubmission};
use argentum_store::{
    CommandClaim, CommandClaimInput, CommandIdentityMatch, CommandIdentityState,
    CommandOutcomeKind, ForkSessionCommandClaim, NewSessionCommandClaim, StoredCommandIdentity,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinSet;

use crate::protocol::{
    canonical_command, parse_request, ClientPayload, CommandDelivery, CommandDeliveryDisposition,
    CommandIdentityMetadata, ResponseEnvelope, ServerPayload, COMMAND_CANONICALIZATION_VERSION,
};
use crate::CommandHost;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_IN_FLIGHT_COMMANDS: usize = 32;
const RESPONSE_QUEUE_CAPACITY: usize = 256;
const COMMAND_DRAIN_GRACE: Duration = Duration::from_millis(250);

pub async fn serve_jsonl<R, W>(mut host: CommandHost, reader: R, writer: W) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    host.disconnect_approval_responder();
    let server_result =
        serve_jsonl_with_limit(&mut host, reader, writer, MAX_IN_FLIGHT_COMMANDS).await;
    let shutdown_result = host.shutdown().await.map_err(std::io::Error::other);
    match (server_result, shutdown_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(server_error), Ok(())) => Err(server_error),
        (Ok(()), Err(shutdown_error)) => Err(shutdown_error),
        (Err(server_error), Err(shutdown_error)) => Err(std::io::Error::other(format!(
            "server failed: {server_error}; runtime shutdown failed: {shutdown_error}"
        ))),
    }
}

async fn serve_jsonl_with_limit<R, W>(
    host: &mut CommandHost,
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
        .send(OutboundResponse::new(
            None,
            ServerPayload::Ready { workspace },
        ))
        .await
        .map_err(channel_closed)?;

    let writer_task = tokio::spawn(async move {
        let mut sequence = 0_u64;
        while let Some(response) = response_receiver.recv().await {
            sequence = sequence.saturating_add(1);
            let mut envelope =
                ResponseEnvelope::new(sequence, response.request_id, response.payload);
            if let Some(delivery) = response.delivery {
                envelope = envelope.with_delivery(delivery);
            }
            write_response(&mut writer, &envelope).await?;
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
                            Ok(event) => ServerPayload::event(event),
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

    let permits = Arc::new(Semaphore::new(max_in_flight.max(1)));
    let active_request_ids = Arc::new(tokio::sync::Mutex::new(HashSet::<String>::new()));
    let mut commands = JoinSet::new();
    let mut buffer = Vec::with_capacity(4096);

    let server_result: std::io::Result<()> = async {
        if let Err(error_value) = host.publish_initial_state() {
            responses
                .send(OutboundResponse::new(
                    None,
                    ServerPayload::error(
                        "initialization_failed",
                        error_value.to_string(),
                        true,
                    ),
                ))
                .await
                .map_err(channel_closed)?;
        }

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
                    .send(OutboundResponse::new(
                        None,
                        ServerPayload::error(
                            "request_too_large",
                            format!("request exceeds the {MAX_REQUEST_BYTES} byte limit"),
                            true,
                        ),
                    ))
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
            let request = match parse_request(&buffer) {
                Ok(request) => request,
                Err(error_value) => {
                    responses
                        .send(OutboundResponse::new(
                            None,
                            ServerPayload::error(
                                "invalid_json",
                                format!("invalid request: {error_value}"),
                                true,
                            ),
                        ))
                        .await
                        .map_err(channel_closed)?;
                    continue;
                }
            };
            let request_id = request.request_id.clone();
            if let Err(error_value) = request.validate() {
                responses
                    .send(OutboundResponse::new(
                        Some(request_id),
                        ServerPayload::error("invalid_request", error_value.to_string(), true),
                    ))
                    .await
                    .map_err(channel_closed)?;
                continue;
            }

            match request.payload {
                ClientPayload::Ping => {
                    let existing = match retained_identity(host, &request_id, &request_id) {
                        Ok(existing) => existing,
                        Err(response) => {
                            responses.send(*response).await.map_err(channel_closed)?;
                            continue;
                        }
                    };
                    if existing.is_some() {
                        responses
                            .send(identity_conflict_response(request_id))
                            .await
                            .map_err(channel_closed)?;
                    } else {
                        responses
                            .send(OutboundResponse::new(
                                Some(request_id),
                                ServerPayload::Pong,
                            ))
                            .await
                            .map_err(channel_closed)?;
                    }
                }
                ClientPayload::InspectCommand { command_request_id } => {
                    let existing = match retained_identity(host, &request_id, &request_id) {
                        Ok(existing) => existing,
                        Err(response) => {
                            responses.send(*response).await.map_err(channel_closed)?;
                            continue;
                        }
                    };
                    if existing.is_some() {
                        responses
                            .send(identity_conflict_response(request_id))
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }
                    let inspected = match retained_identity(host, &command_request_id, &request_id) {
                        Ok(inspected) => inspected,
                        Err(response) => {
                            responses.send(*response).await.map_err(channel_closed)?;
                            continue;
                        }
                    };
                    let payload = match inspected {
                        Some(record) => ServerPayload::CommandIdentityStatus {
                            command: command_metadata(&record),
                        },
                        None => ServerPayload::error(
                            "command_identity_not_found",
                            "no retained command identity exists in this project",
                            true,
                        ),
                    };
                    responses
                        .send(OutboundResponse::new(Some(request_id), payload))
                        .await
                        .map_err(channel_closed)?;
                }
                ClientPayload::Command { command } => {
                    let protected_kind = protected_command_kind(&command);
                    let existing = match retained_identity(host, &request_id, &request_id) {
                        Ok(existing) => existing,
                        Err(response) => {
                            responses.send(*response).await.map_err(channel_closed)?;
                            continue;
                        }
                    };
                    if protected_kind.is_none() && existing.is_some() {
                        responses
                            .send(identity_conflict_response(request_id))
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }
                    if protected_kind.is_some()
                        && active_request_ids.lock().await.contains(&request_id)
                    {
                        responses
                            .send(OutboundResponse::new(
                                Some(request_id),
                                ServerPayload::error(
                                    "duplicate_request_id",
                                    "request_id is already active for a fresh read-only command",
                                    true,
                                ),
                            ))
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }
                    if !jsonl_command_is_permitted(&command) {
                        responses
                            .send(OutboundResponse::new(
                                Some(request_id),
                                ServerPayload::error(
                                    "command_not_permitted",
                                    "command is not permitted on the untrusted JSONL transport",
                                    false,
                                ),
                            ))
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }

                    let Some(command_kind) = protected_kind else {
                        dispatch_unprotected_command(
                            host,
                            command,
                            request_id,
                            &responses,
                            &permits,
                            &active_request_ids,
                            &mut commands,
                        )
                        .await?;
                        continue;
                    };

                    let canonical = match canonical_command(&command) {
                        Ok(canonical) => canonical,
                        Err(error) => {
                            responses
                                .send(OutboundResponse::new(
                                    Some(request_id),
                                    ServerPayload::error(
                                        "invalid_request",
                                        error.to_string(),
                                        false,
                                    ),
                                ))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                    };
                    match durable_command_match(host, &request_id, &canonical) {
                        Err(response) => {
                            responses.send(*response).await.map_err(channel_closed)?;
                            continue;
                        }
                        Ok(CommandIdentityMatch::Equal(record)) => {
                            responses
                                .send(duplicate_response(request_id, *record))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                        Ok(CommandIdentityMatch::Conflict) => {
                            responses
                                .send(identity_conflict_response(request_id))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                        Ok(CommandIdentityMatch::UnsupportedVersion { stored_version }) => {
                            responses
                                .send(unsupported_identity_response(request_id, stored_version))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                        Ok(CommandIdentityMatch::Missing) => {}
                    }

                    let permit = if is_long_running(&command) {
                        match permits.clone().try_acquire_owned() {
                            Ok(permit) => Some(permit),
                            Err(_) => {
                                responses
                                    .send(OutboundResponse::new(
                                        Some(request_id),
                                        ServerPayload::error(
                                            "server_busy",
                                            "the command concurrency limit is active; retry this unaccepted request",
                                            true,
                                        ),
                                    ))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                        }
                    } else {
                        None
                    };

                    let mut scope = match host.resolve_durable_command_scope(&command) {
                        Ok(scope) => scope,
                        Err(error) => {
                            responses
                                .send(OutboundResponse::new(
                                    Some(request_id),
                                    ServerPayload::host_command_failed(&error),
                                ))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                    };
                    let bound_submission = if let AppCommand::SubmitTask { prompt }
                    | AppCommand::SubmitTaskWithSkill { prompt, .. }
                    | AppCommand::SubmitTaskWithAnswerSchema { prompt, .. } = &command
                    {
                        let task_id = argentum_domain::TaskId::new_v4();
                        let run_id = argentum_domain::RunId::new_v4();
                        scope.task_id = Some(task_id);
                        scope.run_id = Some(run_id);
                        let skill = match &command {
                            AppCommand::SubmitTaskWithSkill { skill, .. } => {
                                match host.select_skill(skill) {
                                    Ok(skill) => Some(skill),
                                    Err(error) => {
                                        responses
                                            .send(OutboundResponse::new(
                                                Some(request_id.clone()),
                                                ServerPayload::host_command_failed(&error),
                                            ))
                                            .await
                                            .map_err(channel_closed)?;
                                        continue;
                                    }
                                }
                            }
                            _ => None,
                        };
                        let answer_contract = match &command {
                            AppCommand::SubmitTaskWithAnswerSchema {
                                answer_schema,
                                expected_sha256,
                                ..
                            } => match host.runtime
                                .select_answer_schema(answer_schema, expected_sha256.as_ref())
                            {
                                Ok(contract) => Some(contract),
                                Err(error) => {
                                    let error = crate::HostError::from(error);
                                    responses
                                        .send(OutboundResponse::new(
                                            Some(request_id.clone()),
                                            ServerPayload::host_command_failed(&error),
                                        ))
                                        .await
                                        .map_err(channel_closed)?;
                                    continue;
                                }
                            },
                            _ => None,
                        };
                        Some(TaskSubmission {
                            id: task_id,
                            run_id,
                            target: SubmissionTarget {
                                project_id: host.project_id(),
                                session_id: scope.session_id.expect("submission scope has session"),
                                workspace: host.workspace_root().to_path_buf(),
                            },
                            prompt: prompt.clone(),
                            skill,
                            answer_contract,
                        })
                    } else {
                        None
                    };
                    let input = CommandClaimInput {
                        project_id: host.project_id(),
                        request_id: request_id.clone(),
                        canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
                        canonical_command: canonical.clone(),
                        command_kind: command_kind.into(),
                        scope,
                    };

                    if matches!(command, AppCommand::NewSession) {
                        let claim = match host.claim_durable_new_session(&input) {
                            Ok(claim) => claim,
                            Err(error) => {
                                responses
                                    .send(command_claim_error_response(request_id, &error))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                        };
                        let record = match claim {
                            NewSessionCommandClaim::Claimed { identity, .. } => identity,
                            NewSessionCommandClaim::Existing(record) => {
                                responses
                                    .send(duplicate_response(request_id, record))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                            NewSessionCommandClaim::Conflict => {
                                responses
                                    .send(identity_conflict_response(request_id))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                            NewSessionCommandClaim::UnsupportedVersion { stored_version } => {
                                responses
                                    .send(unsupported_identity_response(request_id, stored_version))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                        };
                        responses
                            .send(
                                OutboundResponse::new(
                                    Some(request_id.clone()),
                                    ServerPayload::CommandAccepted,
                                )
                                .with_delivery(command_delivery(
                                    CommandDeliveryDisposition::Original,
                                    Some(&record),
                                )),
                            )
                            .await
                            .map_err(channel_closed)?;
                        responses
                            .send(
                                OutboundResponse::new(
                                    Some(request_id),
                                    ServerPayload::CommandCompleted,
                                )
                                .with_delivery(command_delivery(
                                    CommandDeliveryDisposition::Original,
                                    Some(&record),
                                )),
                            )
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }

                    if let AppCommand::ForkSession {
                        title,
                        expected_last_message_id,
                        ..
                    } = &command
                    {
                        let claim = match host.claim_durable_fork_session(
                            &input,
                            title,
                            *expected_last_message_id,
                        ) {
                            Ok(claim) => claim,
                            Err(error) => {
                                responses
                                    .send(command_claim_error_response(request_id, &error))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                        };
                        let record = match claim {
                            ForkSessionCommandClaim::Claimed { identity, .. } => identity,
                            ForkSessionCommandClaim::Existing(record) => {
                                responses
                                    .send(duplicate_response(request_id, record))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                            ForkSessionCommandClaim::Conflict => {
                                responses
                                    .send(identity_conflict_response(request_id))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                            ForkSessionCommandClaim::UnsupportedVersion { stored_version } => {
                                responses
                                    .send(unsupported_identity_response(request_id, stored_version))
                                    .await
                                    .map_err(channel_closed)?;
                                continue;
                            }
                        };
                        responses
                            .send(
                                OutboundResponse::new(
                                    Some(request_id.clone()),
                                    ServerPayload::CommandAccepted,
                                )
                                .with_delivery(command_delivery(
                                    CommandDeliveryDisposition::Original,
                                    Some(&record),
                                )),
                            )
                            .await
                            .map_err(channel_closed)?;
                        responses
                            .send(
                                OutboundResponse::new(
                                    Some(request_id),
                                    ServerPayload::CommandCompleted,
                                )
                                .with_delivery(command_delivery(
                                    CommandDeliveryDisposition::Original,
                                    Some(&record),
                                )),
                            )
                            .await
                            .map_err(channel_closed)?;
                        continue;
                    }

                    let claim = match host.claim_durable_command(&input) {
                        Ok(claim) => claim,
                        Err(error) => {
                            responses
                                .send(command_claim_error_response(request_id, &error))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                    };
                    let record = match claim {
                        CommandClaim::Claimed(record) => record,
                        CommandClaim::Existing(record) => {
                            responses
                                .send(duplicate_response(request_id, record))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                        CommandClaim::Conflict => {
                            responses
                                .send(identity_conflict_response(request_id))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                        CommandClaim::UnsupportedVersion { stored_version } => {
                            responses
                                .send(unsupported_identity_response(request_id, stored_version))
                                .await
                                .map_err(channel_closed)?;
                            continue;
                        }
                    };
                    let bound_scope = record.scope.clone();
                    responses
                        .send(
                            OutboundResponse::new(
                                Some(request_id.clone()),
                                ServerPayload::CommandAccepted,
                            )
                            .with_delivery(command_delivery(
                                CommandDeliveryDisposition::Original,
                                Some(&record),
                            )),
                        )
                        .await
                        .map_err(channel_closed)?;

                    let client = host.client();
                    let Some(permit) = permit else {
                        let outbound = execute_protected_command(
                            client,
                            command,
                            bound_submission,
                            bound_scope,
                            request_id,
                            canonical,
                        )
                        .await;
                        responses.send(outbound).await.map_err(channel_closed)?;
                        continue;
                    };
                    let sender = responses.clone();
                    commands.spawn(async move {
                        let _permit = permit;
                        let outbound = execute_protected_command(
                            client,
                            command,
                            bound_submission,
                            bound_scope,
                            request_id,
                            canonical,
                        )
                        .await;
                        let _ = sender.send(outbound).await;
                    });
                }
            }
        }
        Ok(())
    }
    .await;

    let command_result =
        drain_commands_before_shutdown(host, &mut commands, server_result.is_ok()).await;
    let _ = event_shutdown.send(());
    let event_result = event_task.await.map(|_| ()).map_err(std::io::Error::other);
    drop(responses);
    let writer_result = match writer_task.await {
        Ok(result) => result,
        Err(error) => Err(std::io::Error::other(error)),
    };
    let server_result = combine_io_results(server_result, command_result, "command cleanup");
    let server_result = combine_io_results(server_result, event_result, "event forwarder");
    combine_io_results(server_result, writer_result, "response writer")
}

fn combine_io_results(
    primary: std::io::Result<()>,
    secondary: std::io::Result<()>,
    secondary_label: &str,
) -> std::io::Result<()> {
    match (primary, secondary) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(primary), Err(secondary)) => Err(std::io::Error::other(format!(
            "server failed: {primary}; {secondary_label} also failed: {secondary}"
        ))),
    }
}

async fn drain_commands_before_shutdown(
    host: &mut CommandHost,
    commands: &mut JoinSet<()>,
    allow_grace: bool,
) -> std::io::Result<()> {
    let mut first_error = None;
    if allow_grace && !commands.is_empty() {
        let grace = tokio::time::sleep(COMMAND_DRAIN_GRACE);
        tokio::pin!(grace);
        while !commands.is_empty() {
            tokio::select! {
                result = commands.join_next() => {
                    if let Some(Err(error)) = result {
                        first_error.get_or_insert_with(|| std::io::Error::other(error));
                    }
                }
                _ = &mut grace => break,
            }
        }
    }

    let shutdown_request = host.request_shutdown().map_err(std::io::Error::other);
    while let Some(result) = commands.join_next().await {
        if let Err(error) = result {
            first_error.get_or_insert_with(|| std::io::Error::other(error));
        }
    }
    let command_result = first_error.map_or(Ok(()), Err);
    combine_io_results(shutdown_request, command_result, "command task drain")
}

fn jsonl_command_is_permitted(command: &AppCommand) -> bool {
    match command {
        AppCommand::NewSession
        | AppCommand::ForkSession { .. }
        | AppCommand::SelectSession { .. }
        | AppCommand::ListSessions { .. }
        | AppCommand::InspectSession { .. }
        | AppCommand::SetSessionTitle { .. }
        | AppCommand::SetSessionPinned { .. }
        | AppCommand::SetSessionArchived { .. }
        | AppCommand::SetGoal { .. }
        | AppCommand::PauseGoal
        | AppCommand::ResumeGoal
        | AppCommand::ClearGoal
        | AppCommand::ProbeProvider { .. }
        | AppCommand::ListProviderProfiles
        | AppCommand::ListProviderModels { .. }
        | AppCommand::LoadOnboarding
        | AppCommand::ListHarnessState
        | AppCommand::ListApprovalGrants
        | AppCommand::ListExecutionAudit { .. }
        | AppCommand::ListVerificationRunners
        | AppCommand::InspectVerificationRunners
        | AppCommand::LoadVerificationEvidence { .. }
        | AppCommand::SetSurfaceVisibility { .. }
        | AppCommand::SetInspectorSurface { .. }
        | AppCommand::LoadTrajectory { .. }
        | AppCommand::LoadChangeSet { .. }
        | AppCommand::LoadUnifiedDiff { .. }
        | AppCommand::RequestRestore { .. }
        | AppCommand::LoadChangeRecovery
        | AppCommand::RetryChangeRecovery { .. }
        | AppCommand::RequestRecoveryRestore { .. }
        | AppCommand::SubmitTask { .. }
        | AppCommand::SubmitTaskWithSkill { .. }
        | AppCommand::SubmitTaskWithAnswerSchema { .. }
        | AppCommand::CancelRun { .. }
        | AppCommand::ToggleSurface { .. }
        | AppCommand::SetLayout { .. }
        | AppCommand::ResetLayout => true,
        AppCommand::SaveProviderProfile { .. }
        | AppCommand::AddProviderPreset { .. }
        | AppCommand::SelectProviderProfile { .. }
        | AppCommand::SelectProviderModel { .. }
        | AppCommand::SelectHarnessProfile { .. }
        | AppCommand::SelectExecutionProfile { .. }
        | AppCommand::SetHarnessCapabilityEnabled { .. }
        | AppCommand::SetHarnessModule { .. }
        | AppCommand::SetLocalBackendEnabled { .. }
        | AppCommand::BeginOnboarding
        | AppCommand::SetOnboardingGuidance { .. }
        | AppCommand::SetOnboardingProvider { .. }
        | AppCommand::SetOnboardingModel { .. }
        | AppCommand::SetOnboardingWorkspace { .. }
        | AppCommand::SetOnboardingPermission { .. }
        | AppCommand::CompleteOnboarding
        | AppCommand::RequestTool { .. }
        | AppCommand::RevokeApprovalGrant { .. }
        | AppCommand::ApproveTool { .. }
        | AppCommand::ApproveProjectVerification { .. }
        | AppCommand::RejectTool { .. } => false,
    }
}

fn protected_command_kind(command: &AppCommand) -> Option<&'static str> {
    match command {
        AppCommand::NewSession => Some("new_session"),
        AppCommand::ForkSession { .. } => Some("fork_session"),
        AppCommand::SelectSession { .. } => Some("select_session"),
        AppCommand::SetSessionTitle { .. } => Some("set_session_title"),
        AppCommand::SetSessionPinned { .. } => Some("set_session_pinned"),
        AppCommand::SetSessionArchived { .. } => Some("set_session_archived"),
        AppCommand::SetGoal { .. } => Some("set_goal"),
        AppCommand::PauseGoal => Some("pause_goal"),
        AppCommand::ResumeGoal => Some("resume_goal"),
        AppCommand::ClearGoal => Some("clear_goal"),
        AppCommand::ProbeProvider { .. } => Some("probe_provider"),
        AppCommand::SetSurfaceVisibility { .. } => Some("set_surface_visibility"),
        AppCommand::SetInspectorSurface { .. } => Some("set_inspector_surface"),
        AppCommand::RequestRestore { .. } => Some("request_restore"),
        AppCommand::RetryChangeRecovery { .. } => Some("retry_change_recovery"),
        AppCommand::RequestRecoveryRestore { .. } => Some("request_recovery_restore"),
        AppCommand::SubmitTask { .. }
        | AppCommand::SubmitTaskWithSkill { .. }
        | AppCommand::SubmitTaskWithAnswerSchema { .. } => Some("submit_task"),
        AppCommand::CancelRun { .. } => Some("cancel_run"),
        AppCommand::ToggleSurface { .. } => Some("toggle_surface"),
        AppCommand::SetLayout { .. } => Some("set_layout"),
        AppCommand::ResetLayout => Some("reset_layout"),
        _ => None,
    }
}

async fn dispatch_unprotected_command(
    host: &CommandHost,
    command: AppCommand,
    request_id: String,
    responses: &mpsc::Sender<OutboundResponse>,
    permits: &Arc<Semaphore>,
    active_request_ids: &Arc<tokio::sync::Mutex<HashSet<String>>>,
    commands: &mut JoinSet<()>,
) -> std::io::Result<()> {
    {
        let mut active = active_request_ids.lock().await;
        if !active.insert(request_id.clone()) {
            drop(active);
            responses
                .send(OutboundResponse::new(
                    Some(request_id),
                    ServerPayload::error(
                        "duplicate_request_id",
                        "request_id is already active for a fresh read-only command",
                        true,
                    ),
                ))
                .await
                .map_err(channel_closed)?;
            return Ok(());
        }
    }
    let client = host.client();
    let permit = if is_long_running(&command) {
        match permits.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                responses
                    .send(OutboundResponse::new(
                        Some(request_id.clone()),
                        ServerPayload::error(
                            "server_busy",
                            "the command concurrency limit is active; retry this read-only request",
                            true,
                        ),
                    ))
                    .await
                    .map_err(channel_closed)?;
                active_request_ids.lock().await.remove(&request_id);
                return Ok(());
            }
        }
    } else {
        None
    };
    responses
        .send(OutboundResponse::new(
            Some(request_id.clone()),
            ServerPayload::CommandAccepted,
        ))
        .await
        .map_err(channel_closed)?;
    let Some(permit) = permit else {
        let payload = command_result(client.dispatch(command).await);
        responses
            .send(OutboundResponse::new(Some(request_id.clone()), payload))
            .await
            .map_err(channel_closed)?;
        active_request_ids.lock().await.remove(&request_id);
        return Ok(());
    };
    let sender = responses.clone();
    let active_request_ids = active_request_ids.clone();
    commands.spawn(async move {
        let _permit = permit;
        let payload = command_result(client.dispatch(command).await);
        let _ = sender
            .send(OutboundResponse::new(Some(request_id.clone()), payload))
            .await;
        active_request_ids.lock().await.remove(&request_id);
    });
    Ok(())
}

async fn execute_protected_command(
    client: crate::InProcessClient,
    command: AppCommand,
    bound_submission: Option<TaskSubmission>,
    bound_scope: argentum_store::CommandIdentityScope,
    request_id: String,
    canonical_command: Vec<u8>,
) -> OutboundResponse {
    if client
        .mark_durable_command_executing(
            &request_id,
            COMMAND_CANONICALIZATION_VERSION,
            &canonical_command,
        )
        .is_err()
    {
        return unresolved_command_response(
            request_id.clone(),
            client.durable_command_identity(&request_id).ok().flatten(),
            "command_dispatch_not_started",
            "durable command execution could not be entered; the retained identity was not redispatched",
        );
    }

    let validation = client.validate_durable_command_scope(&command, &bound_scope);
    let payload = if let Err(error) = validation {
        command_result(Err(error))
    } else if let Some(submission) = bound_submission {
        let completion = client
            .submit_durable_bound_task(submission, &bound_scope)
            .await;
        if completion.terminal_lifecycle == Some(argentum_domain::TaskLifecycle::Cancelled) {
            ServerPayload::command_failed(
                "command_cancelled",
                "the original task was cancelled before completion",
                false,
            )
        } else {
            command_result(completion.result.map_err(crate::HostError::from))
        }
    } else {
        command_result(client.dispatch_durable_command(command, bound_scope).await)
    };
    let retained_payload = bounded_terminal_payload(payload.clone());
    let outcome_kind = outcome_kind_for_payload(&retained_payload);
    let retained = serde_json::to_string(&retained_payload).ok();
    let settlement = retained.as_deref().map_or_else(
        || {
            Err(crate::HostError::Store(
                argentum_store::StoreError::CommandResultTooLarge,
            ))
        },
        |retained| {
            client.settle_durable_command(
                &request_id,
                COMMAND_CANONICALIZATION_VERSION,
                &canonical_command,
                outcome_kind,
                Some(retained),
            )
        },
    );
    match settlement {
        Ok(record) => OutboundResponse::new(Some(request_id), payload).with_delivery(
            command_delivery(CommandDeliveryDisposition::Original, Some(&record)),
        ),
        Err(_) => unresolved_command_response(
            request_id.clone(),
            client.durable_command_identity(&request_id).ok().flatten(),
            "command_result_unresolved",
            "the command ran but its result could not be durably settled; it will not be repeated",
        ),
    }
}

fn bounded_terminal_payload(payload: ServerPayload) -> ServerPayload {
    match payload {
        ServerPayload::CommandFailed {
            code,
            message,
            recoverable,
            run_id,
            tool_call_id,
            approval_id: _,
        } => ServerPayload::CommandFailed {
            code,
            message: if contains_credential_like_content(&message) {
                "command failed; credential-like detail was not retained".into()
            } else {
                bounded_text(&message, 4096)
            },
            recoverable,
            run_id,
            tool_call_id,
            // Historical delivery never restores an actionable approval ticket.
            approval_id: None,
        },
        payload => payload,
    }
}

fn contains_credential_like_content(value: &str) -> bool {
    let folded = value.to_ascii_lowercase();
    [
        "authorization:",
        "bearer ",
        "api_key",
        "api-key",
        "api key",
        "client_secret",
        "client-secret",
        "client secret",
        "access_token",
        "access-token",
        "access token",
        "refresh_token",
        "refresh-token",
        "refresh token",
        "secret_access_key",
        "secret-access-key",
        "secret access key",
        "private_key",
        "private-key",
        "private key",
        "-----begin private key-----",
        "password=",
        "password:",
    ]
    .iter()
    .any(|marker| folded.contains(marker))
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn outcome_kind_for_payload(payload: &ServerPayload) -> CommandOutcomeKind {
    match payload {
        ServerPayload::CommandCompleted => CommandOutcomeKind::Success,
        ServerPayload::CommandFailed { code, .. }
            if matches!(code.as_str(), "approval_required" | "approval_denied") =>
        {
            CommandOutcomeKind::Denied
        }
        ServerPayload::CommandFailed { code, .. } if code == "command_cancelled" => {
            CommandOutcomeKind::Cancelled
        }
        ServerPayload::CommandFailed { .. } => CommandOutcomeKind::Failure,
        _ => CommandOutcomeKind::Unknown,
    }
}

fn duplicate_response(request_id: String, record: StoredCommandIdentity) -> OutboundResponse {
    match record.state {
        CommandIdentityState::Accepted | CommandIdentityState::Executing => {
            OutboundResponse::new(
                Some(request_id),
                ServerPayload::error(
                    "command_in_progress",
                    "the original durable command is still in progress; no second dispatch occurred",
                    true,
                ),
            )
            .with_delivery(command_delivery(
                CommandDeliveryDisposition::InProgress,
                Some(&record),
            ))
        }
        CommandIdentityState::Interrupted => OutboundResponse::new(
            Some(request_id),
            ServerPayload::error(
                "command_interrupted",
                "the original durable command was interrupted and will not be redispatched",
                false,
            ),
        )
        .with_delivery(command_delivery(
            CommandDeliveryDisposition::Interrupted,
            Some(&record),
        )),
        CommandIdentityState::Unknown => OutboundResponse::new(
            Some(request_id),
            ServerPayload::error(
                "command_outcome_unknown",
                "the original durable command outcome is unknown and will not be redispatched",
                false,
            ),
        )
        .with_delivery(command_delivery(
            CommandDeliveryDisposition::Unknown,
            Some(&record),
        )),
        CommandIdentityState::Settled if !record.replay_allowed => OutboundResponse::new(
            Some(request_id),
            ServerPayload::error(
                "command_result_restricted",
                "the retained command identity is visible but its result is not currently disclosable",
                false,
            ),
        )
        .with_delivery(command_delivery(
            CommandDeliveryDisposition::Restricted,
            Some(&record),
        )),
        CommandIdentityState::Settled if !record.result_available => OutboundResponse::new(
            Some(request_id),
            ServerPayload::error(
                "command_result_unavailable",
                "the retained command identity remains bound but its result body is unavailable",
                false,
            ),
        )
        .with_delivery(command_delivery(
            CommandDeliveryDisposition::ResultUnavailable,
            Some(&record),
        )),
        CommandIdentityState::Settled => {
            let payload = replay_payload(&record);
            match payload {
                Some(payload) => OutboundResponse::new(Some(request_id), payload).with_delivery(
                    command_delivery(CommandDeliveryDisposition::Replayed, Some(&record)),
                ),
                None => OutboundResponse::new(
                    Some(request_id),
                    ServerPayload::error(
                        "command_result_unavailable",
                        "the retained command identity remains bound but its result body is unavailable",
                        false,
                    ),
                )
                .with_delivery(command_delivery(
                    CommandDeliveryDisposition::ResultUnavailable,
                    Some(&record),
                )),
            }
        }
    }
}

fn replay_payload(record: &StoredCommandIdentity) -> Option<ServerPayload> {
    if let Some(encoded) = record.result_payload.as_deref() {
        let payload =
            bounded_terminal_payload(serde_json::from_str::<ServerPayload>(encoded).ok()?);
        let terminal = matches!(
            &payload,
            ServerPayload::CommandCompleted | ServerPayload::CommandFailed { .. }
        );
        return (terminal && Some(outcome_kind_for_payload(&payload)) == record.outcome_kind)
            .then_some(payload);
    }

    let has_authoritative_success_reference =
        (matches!(record.command_kind.as_str(), "new_session" | "fork_session")
            && record.result_session_id.is_some())
            || (record.command_kind == "submit_task"
                && record.scope.run_id.is_some()
                && record.scope.task_id.is_some()
                && record.recovery_revision.is_some());
    if has_authoritative_success_reference
        && record.outcome_kind == Some(CommandOutcomeKind::Success)
    {
        return Some(ServerPayload::CommandCompleted);
    }

    let has_authoritative_terminal_run = record.command_kind == "submit_task"
        && record.scope.run_id.is_some()
        && record.scope.task_id.is_some()
        && record.recovery_revision.is_some();
    if !has_authoritative_terminal_run {
        return None;
    }
    match record.outcome_kind {
        Some(CommandOutcomeKind::Cancelled) => Some(ServerPayload::CommandFailed {
            code: "command_cancelled".into(),
            message:
                "the original task was durably cancelled before its command result was settled"
                    .into(),
            recoverable: false,
            run_id: record.scope.run_id,
            tool_call_id: None,
            approval_id: None,
        }),
        Some(CommandOutcomeKind::Failure) => Some(ServerPayload::CommandFailed {
            code: "command_execution_failed_after_recovery".into(),
            message:
                "the original task durably failed before its command result detail was settled"
                    .into(),
            recoverable: false,
            run_id: record.scope.run_id,
            tool_call_id: None,
            approval_id: None,
        }),
        _ => None,
    }
}

fn identity_conflict_response(request_id: String) -> OutboundResponse {
    OutboundResponse::new(
        Some(request_id),
        ServerPayload::error(
            "command_identity_conflict",
            "request_id is already bound to different durable command input in this project",
            false,
        ),
    )
    .with_delivery(command_delivery(CommandDeliveryDisposition::Conflict, None))
}

fn command_claim_error_response(request_id: String, error: &crate::HostError) -> OutboundResponse {
    let store_error = match error {
        crate::HostError::Store(error) => Some(error),
        crate::HostError::Runtime(argentum_runtime::RuntimeError::Store(error)) => Some(error),
        _ => None,
    };
    let (code, message, recoverable) = match store_error {
        Some(argentum_store::StoreError::CommandIdentityQuotaExceeded) => (
            "command_identity_quota_exceeded",
            "the project command identity quota is full; existing identities remain bound",
            false,
        ),
        Some(argentum_store::StoreError::CommandAdmissionLimit) => (
            "command_admission_limit",
            "the project concurrent durable-command admission limit is active",
            true,
        ),
        _ => (
            "command_identity_unavailable",
            "durable command admission could not be confirmed; inspect or repeat only the same identity",
            true,
        ),
    };
    OutboundResponse::new(
        Some(request_id),
        ServerPayload::error(code, message, recoverable),
    )
}

fn unsupported_identity_response(request_id: String, stored_version: u16) -> OutboundResponse {
    OutboundResponse::new(
        Some(request_id),
        ServerPayload::error(
            "command_identity_version_unsupported",
            format!(
                "the retained command uses canonicalization version {stored_version}, which this build cannot replay"
            ),
            false,
        ),
    )
    .with_delivery(command_delivery(
        CommandDeliveryDisposition::Unknown,
        None,
    ))
}

fn unresolved_command_response(
    request_id: String,
    record: Option<StoredCommandIdentity>,
    code: &'static str,
    message: &'static str,
) -> OutboundResponse {
    OutboundResponse::new(Some(request_id), ServerPayload::error(code, message, false))
        .with_delivery(command_delivery(
            CommandDeliveryDisposition::Unknown,
            record.as_ref(),
        ))
}

fn command_delivery(
    disposition: CommandDeliveryDisposition,
    record: Option<&StoredCommandIdentity>,
) -> CommandDelivery {
    CommandDelivery {
        disposition,
        command: record.map(command_metadata),
    }
}

fn command_metadata(record: &StoredCommandIdentity) -> CommandIdentityMetadata {
    CommandIdentityMetadata {
        identity_version: argentum_store::COMMAND_IDENTITY_VERSION,
        canonicalization_version: record.canonicalization_version,
        command_kind: record.command_kind.clone(),
        state: match record.state {
            CommandIdentityState::Accepted => "accepted",
            CommandIdentityState::Executing => "executing",
            CommandIdentityState::Settled => "settled",
            CommandIdentityState::Interrupted => "interrupted",
            CommandIdentityState::Unknown => "unknown",
        }
        .into(),
        outcome_kind: record.outcome_kind.map(|outcome| {
            match outcome {
                CommandOutcomeKind::Success => "success",
                CommandOutcomeKind::Failure => "failure",
                CommandOutcomeKind::Cancelled => "cancelled",
                CommandOutcomeKind::Denied => "denied",
                CommandOutcomeKind::Interrupted => "interrupted",
                CommandOutcomeKind::Unknown => "unknown",
            }
            .into()
        }),
        payload_available: record.result_available,
        replay_eligible: record.replay_allowed
            && record.result_available
            && record.state == CommandIdentityState::Settled,
        accepted_at_unix: record.accepted_at.unix_timestamp(),
        updated_at_unix: record.updated_at.unix_timestamp(),
        session_id: record.scope.session_id,
        run_id: record.scope.run_id,
        task_id: record.scope.task_id,
        subject_id: record.scope.subject_id,
        provider_profile_id: record.scope.provider_profile_id.clone(),
        execution_snapshot_sha256: record.scope.execution_snapshot_sha256.clone(),
        definition_revision: record.scope.definition_revision,
        result_session_id: record.result_session_id,
        recovery_revision: record.recovery_revision,
        recovery_blocker: record.recovery_blocker.clone(),
    }
}

fn retained_identity(
    host: &CommandHost,
    lookup_request_id: &str,
    response_request_id: &str,
) -> Result<Option<StoredCommandIdentity>, Box<OutboundResponse>> {
    host.durable_command_identity(lookup_request_id)
        .map_err(|_| {
            Box::new(identity_lookup_unavailable_response(
                response_request_id.to_owned(),
            ))
        })
}

fn durable_command_match(
    host: &CommandHost,
    request_id: &str,
    canonical_command: &[u8],
) -> Result<CommandIdentityMatch, Box<OutboundResponse>> {
    host.durable_command_match(
        request_id,
        COMMAND_CANONICALIZATION_VERSION,
        canonical_command,
    )
    .map_err(|_| Box::new(identity_lookup_unavailable_response(request_id.to_owned())))
}

fn identity_lookup_unavailable_response(request_id: String) -> OutboundResponse {
    OutboundResponse::new(
        Some(request_id),
        ServerPayload::error(
            "command_identity_unavailable",
            "the retained command identity could not be read safely; no dispatch occurred",
            false,
        ),
    )
    .with_delivery(command_delivery(CommandDeliveryDisposition::Unknown, None))
}

fn is_long_running(command: &AppCommand) -> bool {
    matches!(
        command,
        AppCommand::SubmitTask { .. }
            | AppCommand::SubmitTaskWithSkill { .. }
            | AppCommand::SubmitTaskWithAnswerSchema { .. }
            | AppCommand::ProbeProvider { .. }
            | AppCommand::ListProviderModels { .. }
            | AppCommand::RequestTool { .. }
            | AppCommand::ApproveTool { .. }
            | AppCommand::RequestRestore { .. }
            | AppCommand::RequestRecoveryRestore { .. }
    )
}

fn command_result(result: Result<(), crate::HostError>) -> ServerPayload {
    match result {
        Ok(()) => ServerPayload::CommandCompleted,
        Err(error_value) => ServerPayload::host_command_failed(&error_value),
    }
}

#[derive(Debug)]
struct OutboundResponse {
    request_id: Option<String>,
    payload: ServerPayload,
    delivery: Option<CommandDelivery>,
}

impl OutboundResponse {
    fn new(request_id: Option<String>, payload: ServerPayload) -> Self {
        Self {
            request_id,
            payload,
            delivery: None,
        }
    }

    fn with_delivery(mut self, delivery: CommandDelivery) -> Self {
        self.delivery = Some(delivery);
        self
    }
}

fn event_payload(
    event: Result<argentum_domain::AppEvent, tokio::sync::broadcast::error::RecvError>,
) -> Option<ServerPayload> {
    match event {
        Ok(event) => Some(ServerPayload::event(event)),
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
        .send(OutboundResponse::new(None, payload))
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
    #[test]
    fn jsonl_cannot_change_installed_module_state() {
        assert!(!super::jsonl_command_is_permitted(
            &argentum_domain::AppCommand::AddProviderPreset {
                preset_id: "deepseek".into()
            }
        ));
        assert!(!super::jsonl_command_is_permitted(
            &argentum_domain::AppCommand::SetLocalBackendEnabled { enabled: true }
        ));
        assert!(!super::jsonl_command_is_permitted(
            &argentum_domain::AppCommand::SetLocalBackendEnabled { enabled: false }
        ));
        for action in [
            argentum_domain::ModuleAction::Install,
            argentum_domain::ModuleAction::Enable,
            argentum_domain::ModuleAction::Disable,
            argentum_domain::ModuleAction::Remove,
        ] {
            assert!(!super::jsonl_command_is_permitted(
                &argentum_domain::AppCommand::SetHarnessModule {
                    module_id: "first-party.activity".into(),
                    action,
                }
            ));
        }
    }
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};
    use std::time::Duration;

    use argentum_domain::{
        now, AppCommand, AppEvent, ApprovalScope, ConversationMessage, ConversationMessageStatus,
        ConversationRole, MessageId, RunId, ToolInput, ToolRequest,
    };
    use argentum_providers::{
        ModelProvider, ModelRequest, ModelToolCall, ProviderEvent, ProviderRegistry,
    };
    use argentum_runtime::RuntimeService;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use argentum_store::{CommandIdentityScope, Store};
    use argentum_tools::ToolRegistry;
    use argentum_workspaces::WorkspaceManager;
    use async_trait::async_trait;
    use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, AsyncWrite, BufReader, ReadBuf};
    use tokio::sync::mpsc;

    use super::*;
    use crate::protocol::{RequestEnvelope, PROTOCOL_VERSION};
    use crate::HostConfig;

    #[tokio::test]
    async fn jsonl_server_shutdown_unmounts_the_composed_runtime() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("composed host");
        let observer = host.lifecycle_observer();

        serve_jsonl(
            host,
            tokio::io::BufReader::new(tokio::io::empty()),
            tokio::io::sink(),
        )
        .await
        .expect("server shutdown");

        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("server lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn jsonl_eof_cancels_pending_commands_before_waiting() {
        let (_workspace, store, host) = pending_composed_host().await;
        let project_id = host.project_id();
        let observer = host.lifecycle_observer();
        let input = pending_submit_input();

        tokio::time::timeout(
            Duration::from_secs(1),
            serve_jsonl(host, BufReader::new(input.as_bytes()), tokio::io::sink()),
        )
        .await
        .expect("server stopped after EOF")
        .expect("server shutdown");

        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("server lifecycle")
            .module_ids()
            .is_empty());
        let command = store
            .command_identity(project_id, "pending-cleanup")
            .unwrap()
            .expect("cancelled command identity");
        assert_eq!(command.state, CommandIdentityState::Settled);
        assert_eq!(command.outcome_kind, Some(CommandOutcomeKind::Cancelled));
        assert!(command
            .result_payload
            .as_deref()
            .is_some_and(|payload| payload.contains("command_cancelled")));
        let replay = duplicate_response("pending-cleanup".into(), command);
        assert!(matches!(
            replay.payload,
            ServerPayload::CommandFailed { ref code, .. } if code == "command_cancelled"
        ));
        assert!(replay.delivery.as_ref().is_some_and(|delivery| {
            delivery.disposition == CommandDeliveryDisposition::Replayed
                && delivery
                    .command
                    .as_ref()
                    .is_some_and(|metadata| metadata.outcome_kind.as_deref() == Some("cancelled"))
        }));
    }

    #[tokio::test]
    async fn durable_goal_execution_uses_the_session_bound_at_claim_time() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let original_session = runtime.session_id();
        let command = AppCommand::SetGoal {
            objective: "Remain bound to the accepted session".into(),
            token_budget: Some(100),
            tool_budget: Some(2),
            time_budget_seconds: Some(60),
        };
        let canonical = canonical_command(&command).unwrap();
        let scope = runtime.resolve_durable_command_scope(&command).unwrap();
        let claim = store
            .claim_command_identity(&CommandClaimInput {
                project_id,
                request_id: "bound-goal".into(),
                canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
                canonical_command: canonical.clone(),
                command_kind: "set_goal".into(),
                scope,
            })
            .unwrap();
        let CommandClaim::Claimed(record) = claim else {
            panic!("new durable command claim");
        };

        runtime.dispatch(AppCommand::NewSession).await.unwrap();
        let newly_selected_session = runtime.session_id();
        assert_ne!(newly_selected_session, original_session);
        let response = execute_protected_command(
            crate::InProcessClient {
                runtime: runtime.clone(),
            },
            command,
            None,
            record.scope,
            "bound-goal".into(),
            canonical,
        )
        .await;

        assert!(matches!(response.payload, ServerPayload::CommandCompleted));
        assert_eq!(
            store
                .goal(project_id, original_session)
                .unwrap()
                .expect("goal on accepted session")
                .objective,
            "Remain bound to the accepted session"
        );
        assert!(store
            .goal(project_id, newly_selected_session)
            .unwrap()
            .is_none());
        assert_eq!(runtime.session_id(), newly_selected_session);
    }

    #[tokio::test]
    async fn durable_goal_execution_rejects_a_changed_bound_definition() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let session_id = runtime.session_id();
        runtime
            .dispatch(AppCommand::SetGoal {
                objective: "Original goal definition".into(),
                token_budget: Some(100),
                tool_budget: Some(2),
                time_budget_seconds: Some(60),
            })
            .await
            .unwrap();
        let command = AppCommand::PauseGoal;
        let canonical = canonical_command(&command).unwrap();
        let scope = runtime.resolve_durable_command_scope(&command).unwrap();
        assert!(scope.definition_revision.is_some());
        assert!(scope.execution_snapshot_sha256.is_some());
        let claim = store
            .claim_command_identity(&CommandClaimInput {
                project_id,
                request_id: "changed-goal-snapshot".into(),
                canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
                canonical_command: canonical.clone(),
                command_kind: "pause_goal".into(),
                scope,
            })
            .unwrap();
        let CommandClaim::Claimed(record) = claim else {
            panic!("new durable command claim");
        };

        runtime
            .dispatch(AppCommand::SetGoal {
                objective: "Replacement goal definition".into(),
                token_budget: Some(200),
                tool_budget: Some(3),
                time_budget_seconds: Some(120),
            })
            .await
            .unwrap();
        let response = execute_protected_command(
            crate::InProcessClient {
                runtime: runtime.clone(),
            },
            command,
            None,
            record.scope,
            "changed-goal-snapshot".into(),
            canonical,
        )
        .await;

        assert!(matches!(
            response.payload,
            ServerPayload::CommandFailed { ref code, .. }
                if code == "durable_command_snapshot_changed"
        ));
        let goal = store
            .goal(project_id, session_id)
            .unwrap()
            .expect("replacement goal");
        assert_eq!(goal.objective, "Replacement goal definition");
        assert_eq!(goal.lifecycle, argentum_domain::GoalLifecycle::Active);
        let retained = store
            .command_identity(project_id, "changed-goal-snapshot")
            .unwrap()
            .expect("retained failure");
        assert_eq!(retained.state, CommandIdentityState::Settled);
        assert_eq!(retained.outcome_kind, Some(CommandOutcomeKind::Failure));
    }

    #[tokio::test]
    async fn durable_submission_rejects_changed_provider_snapshot_before_request() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let requests = Arc::new(AtomicUsize::new(0));
        let mut providers = ProviderRegistry::default();
        providers.register(CompletingProvider {
            requests: Arc::clone(&requests),
        });
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            providers,
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let profile = argentum_domain::ProviderProfile {
            id: "f09-complete".into(),
            label: "F09 complete".into(),
            kind: argentum_domain::ProviderKind::OpenAiCompatible,
            endpoint: "https://complete.example.test/v1/".into(),
            model: "accepted-model".into(),
            selected: true,
        };
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: profile.clone(),
            })
            .await
            .unwrap();
        runtime
            .declare_model_budget("f09-complete", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        let project_id = runtime.project_id();
        let session_id = runtime.session_id();
        let command = AppCommand::SubmitTask {
            prompt: "must not reach the changed provider".into(),
        };
        let canonical = canonical_command(&command).unwrap();
        let mut scope = runtime.resolve_durable_command_scope(&command).unwrap();
        let task_id = argentum_domain::TaskId::new_v4();
        let run_id = argentum_domain::RunId::new_v4();
        scope.task_id = Some(task_id);
        scope.run_id = Some(run_id);
        assert_eq!(scope.provider_profile_id.as_deref(), Some("f09-complete"));
        assert!(scope.execution_snapshot_sha256.is_some());
        let claim = store
            .claim_command_identity(&CommandClaimInput {
                project_id,
                request_id: "changed-provider-snapshot".into(),
                canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
                canonical_command: canonical.clone(),
                command_kind: "submit_task".into(),
                scope,
            })
            .unwrap();
        let CommandClaim::Claimed(record) = claim else {
            panic!("new durable command claim");
        };

        let mut changed = profile;
        changed.model = "retargeted-model".into();
        store
            .save_provider_profile(project_id, &changed)
            .expect("change stored profile after acceptance");
        let race_guard = runtime
            .submit_durable_bound_task(
                TaskSubmission {
                    id: task_id,
                    run_id,
                    target: SubmissionTarget {
                        project_id,
                        session_id,
                        workspace: runtime.workspace_root().to_path_buf(),
                    },
                    prompt: "must not reach the changed provider".into(),
                    skill: None,
                    answer_contract: None,
                },
                &record.scope,
            )
            .await;
        assert!(!race_guard.admitted);
        assert!(matches!(
            race_guard.result,
            Err(argentum_runtime::RuntimeError::DurableCommandSnapshotChanged)
        ));
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        let response = execute_protected_command(
            crate::InProcessClient {
                runtime: runtime.clone(),
            },
            command,
            Some(TaskSubmission {
                id: task_id,
                run_id,
                target: SubmissionTarget {
                    project_id,
                    session_id,
                    workspace: runtime.workspace_root().to_path_buf(),
                },
                prompt: "must not reach the changed provider".into(),
                skill: None,
                answer_contract: None,
            }),
            record.scope,
            "changed-provider-snapshot".into(),
            canonical,
        )
        .await;

        assert!(matches!(
            response.payload,
            ServerPayload::CommandFailed { ref code, .. }
                if code == "durable_command_snapshot_changed"
        ));
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(store
            .events_for_workspace(workspace.path())
            .unwrap()
            .iter()
            .all(|event| !matches!(event, AppEvent::TaskAccepted(_))));
        let retained = store
            .command_identity(project_id, "changed-provider-snapshot")
            .unwrap()
            .expect("retained failure");
        assert_eq!(retained.state, CommandIdentityState::Settled);
        assert_eq!(retained.outcome_kind, Some(CommandOutcomeKind::Failure));
    }

    #[tokio::test]
    async fn durable_snapshots_detect_policy_budget_and_probe_profile_changes() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let profile = argentum_domain::ProviderProfile {
            id: "snapshot-profile".into(),
            label: "Snapshot profile".into(),
            kind: argentum_domain::ProviderKind::OpenAiCompatible,
            endpoint: "https://snapshot.example.test/v1/".into(),
            model: "snapshot-model".into(),
            selected: true,
        };
        store
            .save_provider_profile(project_id, &profile)
            .expect("provider profile");
        runtime
            .declare_model_budget("snapshot-profile", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        let submission = AppCommand::SubmitTask {
            prompt: "bind mutable execution inputs".into(),
        };

        let policy_scope = runtime.resolve_durable_command_scope(&submission).unwrap();
        let original_policy = runtime.execution_policy().unwrap();
        let mut changed_policy = original_policy.clone();
        changed_policy.approval_mode = argentum_domain::ApprovalMode::Never;
        store
            .save_execution_policy(project_id, &changed_policy)
            .expect("changed policy");
        assert!(matches!(
            runtime.validate_durable_command_scope(&submission, &policy_scope),
            Err(argentum_runtime::RuntimeError::DurableCommandSnapshotChanged)
        ));

        store
            .save_execution_policy(project_id, &original_policy)
            .expect("restore policy");
        let budget_scope = runtime.resolve_durable_command_scope(&submission).unwrap();
        runtime
            .declare_model_budget("snapshot-profile", [2_000_000, 1_900_000, 8192, 2048, 1024])
            .unwrap();
        assert!(matches!(
            runtime.validate_durable_command_scope(&submission, &budget_scope),
            Err(argentum_runtime::RuntimeError::DurableCommandSnapshotChanged)
        ));

        let probe = AppCommand::ProbeProvider {
            provider_id: "snapshot-profile".into(),
        };
        let probe_scope = runtime.resolve_durable_command_scope(&probe).unwrap();
        let mut changed_profile = profile;
        changed_profile.label = "Changed snapshot profile".into();
        store
            .save_provider_profile(project_id, &changed_profile)
            .expect("changed probe profile");
        assert!(matches!(
            runtime.validate_durable_command_scope(&probe, &probe_scope),
            Err(argentum_runtime::RuntimeError::DurableCommandSnapshotChanged)
        ));
        assert!(matches!(
            runtime
                .probe_provider_with_durable_scope("snapshot-profile".into(), Some(&probe_scope),)
                .await,
            Err(argentum_runtime::RuntimeError::DurableCommandSnapshotChanged)
        ));
    }

    #[tokio::test]
    async fn replay_rejects_nonterminal_mismatched_and_unproved_success_payloads() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let command = AppCommand::PauseGoal;
        let canonical = canonical_command(&command).unwrap();
        let input = CommandClaimInput {
            project_id: runtime.project_id(),
            request_id: "malformed-replay".into(),
            canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
            canonical_command: canonical.clone(),
            command_kind: "pause_goal".into(),
            scope: CommandIdentityScope {
                session_id: Some(runtime.session_id()),
                ..CommandIdentityScope::default()
            },
        };
        store.claim_command_identity(&input).unwrap();
        let mut record = store
            .settle_command_identity(
                input.project_id,
                &input.request_id,
                input.canonicalization_version,
                &canonical,
                CommandOutcomeKind::Failure,
                Some(
                    &serde_json::to_string(&ServerPayload::command_failed(
                        "definite_failure",
                        "retained failure",
                        false,
                    ))
                    .unwrap(),
                ),
            )
            .unwrap();

        record.result_payload = Some(serde_json::to_string(&ServerPayload::Pong).unwrap());
        assert!(matches!(
            duplicate_response("malformed-replay".into(), record.clone()).payload,
            ServerPayload::Error { ref code, .. } if code == "command_result_unavailable"
        ));
        record.result_payload =
            Some(serde_json::to_string(&ServerPayload::CommandCompleted).unwrap());
        assert!(matches!(
            duplicate_response("malformed-replay".into(), record.clone()).payload,
            ServerPayload::Error { ref code, .. } if code == "command_result_unavailable"
        ));
        record.result_payload = None;
        record.outcome_kind = Some(CommandOutcomeKind::Success);
        assert!(matches!(
            duplicate_response("malformed-replay".into(), record).payload,
            ServerPayload::Error { ref code, .. } if code == "command_result_unavailable"
        ));

        let mut reconstructed = store
            .command_identity(input.project_id, &input.request_id)
            .unwrap()
            .unwrap();
        reconstructed.command_kind = "submit_task".into();
        reconstructed.outcome_kind = Some(CommandOutcomeKind::Success);
        reconstructed.result_payload = None;
        reconstructed.scope.run_id = Some(argentum_domain::RunId::new_v4());
        reconstructed.scope.task_id = Some(argentum_domain::TaskId::new_v4());
        reconstructed.recovery_revision = Some(argentum_domain::RunId::new_v4());
        assert!(matches!(
            duplicate_response("malformed-replay".into(), reconstructed.clone()).payload,
            ServerPayload::CommandCompleted
        ));
        reconstructed.outcome_kind = Some(CommandOutcomeKind::Cancelled);
        assert!(matches!(
            duplicate_response("malformed-replay".into(), reconstructed.clone()).payload,
            ServerPayload::CommandFailed { ref code, run_id: Some(_), .. }
                if code == "command_cancelled"
        ));
        reconstructed.outcome_kind = Some(CommandOutcomeKind::Failure);
        assert!(matches!(
            duplicate_response("malformed-replay".into(), reconstructed).payload,
            ServerPayload::CommandFailed { ref code, run_id: Some(_), .. }
                if code == "command_execution_failed_after_recovery"
        ));

        let mut denied = store
            .command_identity(input.project_id, &input.request_id)
            .unwrap()
            .unwrap();
        denied.outcome_kind = Some(CommandOutcomeKind::Denied);
        denied.result_payload = Some(
            serde_json::to_string(&ServerPayload::CommandFailed {
                code: "approval_required".into(),
                message: "password=PRIVATE_REPLAY_CANARY".into(),
                recoverable: false,
                run_id: None,
                tool_call_id: None,
                approval_id: Some(argentum_domain::ApprovalId::new_v4()),
            })
            .unwrap(),
        );
        let denied_replay = duplicate_response("malformed-replay".into(), denied);
        assert!(matches!(
            denied_replay.payload,
            ServerPayload::CommandFailed {
                ref code,
                ref message,
                approval_id: None,
                ..
            } if code == "approval_required" && !message.contains("PRIVATE_REPLAY_CANARY")
        ));
    }

    #[tokio::test]
    async fn jsonl_read_error_drains_pending_command_before_shutdown() {
        let (_workspace, _store, host) = pending_composed_host().await;
        let observer = host.lifecycle_observer();
        let reader = DelayedFailureReader::new(pending_submit_input().into_bytes());

        let error = tokio::time::timeout(
            Duration::from_secs(2),
            serve_jsonl(host, reader, tokio::io::sink()),
        )
        .await
        .expect("server stopped after read failure")
        .expect_err("forced read failure");

        let rendered = error.to_string();
        assert!(rendered.contains("forced test read failure"));
        assert!(!rendered.contains("runtime shutdown failed"));
        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("server lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn jsonl_writer_panic_preserves_the_transport_error() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("composed host");
        let reader = DelayedFailureReader::new(Vec::new());

        let error = serve_jsonl(host, reader, PanickingWriter)
            .await
            .expect_err("combined server failure");
        let rendered = error.to_string();

        assert!(rendered.contains("forced test read failure"));
        assert!(rendered.contains("response writer"));
        assert!(rendered.contains("forced writer panic"));
    }

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
    async fn completed_new_session_identity_replays_one_original_session() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let before = store.workspace_snapshot(project_id).unwrap().sessions.len();
        let host = CommandHost::from_runtime(runtime);
        let request = RequestEnvelope::command("new-session-once", AppCommand::NewSession);

        let responses = serve_requests(host, &[request.clone(), request]).await;
        let after = store.workspace_snapshot(project_id).unwrap();
        assert_eq!(after.sessions.len(), before + 1);
        let terminals = responses
            .iter()
            .filter(|response| {
                response.request_id.as_deref() == Some("new-session-once")
                    && matches!(response.payload, ServerPayload::CommandCompleted)
            })
            .collect::<Vec<_>>();
        assert_eq!(terminals.len(), 2);
        assert_eq!(
            terminals[0]
                .delivery
                .as_ref()
                .map(|delivery| delivery.disposition),
            Some(CommandDeliveryDisposition::Original)
        );
        assert_eq!(
            terminals[1]
                .delivery
                .as_ref()
                .map(|delivery| delivery.disposition),
            Some(CommandDeliveryDisposition::Replayed)
        );
        let original_session = terminals[0]
            .delivery
            .as_ref()
            .and_then(|delivery| delivery.command.as_ref())
            .and_then(|command| command.result_session_id);
        assert_eq!(
            terminals[1]
                .delivery
                .as_ref()
                .and_then(|delivery| delivery.command.as_ref())
                .and_then(|command| command.result_session_id),
            original_session
        );
        assert_eq!(after.active_session_id, original_session);
    }

    #[tokio::test]
    async fn completed_fork_identity_replays_one_child_without_selecting_it() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let source_session_id = runtime.session_id();
        let run_id = RunId::new_v4();
        for (role, text) in [
            (ConversationRole::User, "fork source"),
            (ConversationRole::Assistant, "settled answer"),
        ] {
            store
                .append_conversation_message(&ConversationMessage {
                    id: MessageId::new_v4(),
                    project_id,
                    session_id: source_session_id,
                    run_id,
                    role,
                    text: text.into(),
                    reasoning: String::new(),
                    usage: None,
                    profile_id: if role == ConversationRole::Assistant {
                        "fixture".into()
                    } else {
                        String::new()
                    },
                    model: if role == ConversationRole::Assistant {
                        "fixture".into()
                    } else {
                        String::new()
                    },
                    status: ConversationMessageStatus::Complete,
                    created_at: now(),
                })
                .expect("conversation message");
        }
        let before = store.workspace_snapshot(project_id).unwrap().sessions.len();
        let host = CommandHost::from_runtime(runtime);
        let request = RequestEnvelope::command(
            "fork-once",
            AppCommand::ForkSession {
                source_session_id,
                title: "Focused branch".into(),
                expected_last_message_id: None,
            },
        );
        let responses = serve_requests(host, &[request.clone(), request]).await;
        let after = store.workspace_snapshot(project_id).unwrap();
        assert_eq!(after.sessions.len(), before + 1);
        assert_eq!(after.active_session_id, Some(source_session_id));
        let terminals = responses
            .iter()
            .filter(|response| {
                response.request_id.as_deref() == Some("fork-once")
                    && matches!(response.payload, ServerPayload::CommandCompleted)
            })
            .collect::<Vec<_>>();
        assert_eq!(terminals.len(), 2);
        assert_eq!(
            terminals[0]
                .delivery
                .as_ref()
                .map(|delivery| delivery.disposition),
            Some(CommandDeliveryDisposition::Original)
        );
        assert_eq!(
            terminals[1]
                .delivery
                .as_ref()
                .map(|delivery| delivery.disposition),
            Some(CommandDeliveryDisposition::Replayed)
        );
        let child = terminals[0]
            .delivery
            .as_ref()
            .and_then(|delivery| delivery.command.as_ref())
            .and_then(|command| command.result_session_id)
            .expect("fork child");
        assert_eq!(
            terminals[1]
                .delivery
                .as_ref()
                .and_then(|delivery| delivery.command.as_ref())
                .and_then(|command| command.result_session_id),
            Some(child)
        );
        assert!(store
            .session_fork_snapshot(project_id, child)
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn replayed_session_selection_returns_history_without_retargeting_current_state() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let first_session = runtime.session_id();
        runtime.dispatch(AppCommand::NewSession).await.unwrap();
        let second_session = runtime.session_id();
        runtime
            .dispatch(AppCommand::SelectSession {
                session_id: first_session,
            })
            .await
            .unwrap();
        let responses = serve_requests(
            CommandHost::from_runtime(runtime),
            &[
                RequestEnvelope::command(
                    "select-history",
                    AppCommand::SelectSession {
                        session_id: second_session,
                    },
                ),
                RequestEnvelope::command(
                    "select-current",
                    AppCommand::SelectSession {
                        session_id: first_session,
                    },
                ),
                RequestEnvelope::command(
                    "select-history",
                    AppCommand::SelectSession {
                        session_id: second_session,
                    },
                ),
            ],
        )
        .await;

        let historical = responses
            .iter()
            .filter(|response| {
                response.request_id.as_deref() == Some("select-history")
                    && matches!(response.payload, ServerPayload::CommandCompleted)
            })
            .collect::<Vec<_>>();
        assert_eq!(historical.len(), 2);
        assert_eq!(
            historical[0].delivery.as_ref().unwrap().disposition,
            CommandDeliveryDisposition::Original
        );
        assert_eq!(
            historical[1].delivery.as_ref().unwrap().disposition,
            CommandDeliveryDisposition::Replayed
        );
        assert!(historical.iter().all(|response| {
            response
                .delivery
                .as_ref()
                .and_then(|delivery| delivery.command.as_ref())
                .and_then(|command| command.session_id)
                == Some(second_session)
        }));
        assert_eq!(
            store
                .workspace_snapshot(project_id)
                .unwrap()
                .active_session_id,
            Some(first_session)
        );
    }

    #[tokio::test]
    async fn definite_precondition_failure_stays_failed_after_scope_becomes_admissible() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let session_id = runtime.session_id();
        let pause = RequestEnvelope::command("failed-once", AppCommand::PauseGoal);
        let responses = serve_requests(
            CommandHost::from_runtime(runtime),
            &[
                pause.clone(),
                RequestEnvelope::command(
                    "create-goal",
                    AppCommand::SetGoal {
                        objective: "A later policy state must not reuse an old failure".into(),
                        token_budget: Some(10_000),
                        tool_budget: Some(4),
                        time_budget_seconds: Some(60),
                    },
                ),
                pause,
            ],
        )
        .await;

        let failures = responses
            .iter()
            .filter(|response| {
                response.request_id.as_deref() == Some("failed-once")
                    && matches!(response.payload, ServerPayload::CommandFailed { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 2);
        assert_eq!(
            failures[0].delivery.as_ref().unwrap().disposition,
            CommandDeliveryDisposition::Original
        );
        assert_eq!(
            failures[1].delivery.as_ref().unwrap().disposition,
            CommandDeliveryDisposition::Replayed
        );
        assert_eq!(failures[0].payload, failures[1].payload);
        assert_eq!(
            store
                .goal(project_id, session_id)
                .unwrap()
                .unwrap()
                .lifecycle,
            argentum_domain::GoalLifecycle::Active
        );
    }

    #[tokio::test]
    async fn completed_composed_task_replay_emits_no_queue_driving_events_or_session_retarget() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let requests = Arc::new(AtomicUsize::new(0));
        let mut providers = ProviderRegistry::default();
        providers.register(CompletingProvider {
            requests: Arc::clone(&requests),
        });
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new_composed(
            store.clone(),
            providers,
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .await
        .expect("composed runtime");
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: argentum_domain::ProviderProfile {
                    id: "f09-complete".into(),
                    label: "F09 complete".into(),
                    kind: argentum_domain::ProviderKind::OpenAiCompatible,
                    endpoint: "https://complete.example.test/v1/".into(),
                    model: "test-model".into(),
                    selected: true,
                },
            })
            .await
            .unwrap();
        runtime
            .declare_model_budget("f09-complete", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        let project_id = runtime.project_id();
        let original_session = runtime.session_id();
        let request = RequestEnvelope::command(
            "completed-task",
            AppCommand::SubmitTask {
                prompt: "complete once".into(),
            },
        );
        let original = serve_requests(
            CommandHost::from_runtime(runtime),
            std::slice::from_ref(&request),
        )
        .await;
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert_eq!(
            original
                .iter()
                .filter(|response| matches!(
                    &response.payload,
                    ServerPayload::Event { event }
                        if matches!(event.as_ref(), AppEvent::TaskAccepted(_))
                ))
                .count(),
            1
        );
        let original_binding = original
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("completed-task")
                    && matches!(response.payload, ServerPayload::CommandCompleted)
            })
            .and_then(|response| response.delivery.as_ref())
            .and_then(|delivery| delivery.command.clone())
            .expect("original command binding");

        let mut providers = ProviderRegistry::default();
        providers.register(CompletingProvider {
            requests: Arc::clone(&requests),
        });
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new_composed(
            store.clone(),
            providers,
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .await
        .expect("restarted composed runtime");
        runtime
            .declare_model_budget("f09-complete", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        runtime.dispatch(AppCommand::NewSession).await.unwrap();
        let current_session = runtime.session_id();
        let replay = serve_requests(CommandHost::from_runtime(runtime), &[request]).await;

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(replay.iter().all(|response| !matches!(
            &response.payload,
            ServerPayload::Event { event }
                if matches!(
                    event.as_ref(),
                    AppEvent::TaskAccepted(_)
                        | AppEvent::RunStatusChanged { .. }
                        | AppEvent::GoalInvocationSnapshot { .. }
                )
        )));
        let replay_binding = replay
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("completed-task")
                    && matches!(response.payload, ServerPayload::CommandCompleted)
                    && response.delivery.as_ref().is_some_and(|delivery| {
                        delivery.disposition == CommandDeliveryDisposition::Replayed
                    })
            })
            .and_then(|response| response.delivery.as_ref())
            .and_then(|delivery| delivery.command.clone())
            .expect("replayed command binding");
        assert_eq!(replay_binding.run_id, original_binding.run_id);
        assert_eq!(replay_binding.task_id, original_binding.task_id);
        assert_eq!(replay_binding.session_id, Some(original_session));
        assert_ne!(current_session, original_session);
        assert_eq!(
            store
                .workspace_snapshot(project_id)
                .unwrap()
                .active_session_id,
            Some(current_session)
        );
    }

    #[tokio::test]
    async fn changed_payload_conflicts_without_mutating_or_disclosing_original_input() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let project_id = runtime.project_id();
        let before = store.workspace_snapshot(project_id).unwrap().sessions.len();
        let host = CommandHost::from_runtime(runtime);
        let responses = serve_requests(
            host,
            &[
                RequestEnvelope::command("bound", AppCommand::NewSession),
                RequestEnvelope::command(
                    "bound",
                    AppCommand::SetGoal {
                        objective: "private conflicting objective".into(),
                        token_budget: None,
                        tool_budget: None,
                        time_budget_seconds: None,
                    },
                ),
            ],
        )
        .await;

        assert_eq!(
            store.workspace_snapshot(project_id).unwrap().sessions.len(),
            before + 1
        );
        assert!(store
            .goal(
                project_id,
                store
                    .workspace_snapshot(project_id)
                    .unwrap()
                    .active_session_id
                    .unwrap()
            )
            .unwrap()
            .is_none());
        let conflict = responses
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("bound")
                    && matches!(
                        &response.payload,
                        ServerPayload::Error { code, .. }
                            if code == "command_identity_conflict"
                    )
            })
            .expect("identity conflict");
        let encoded = serde_json::to_string(conflict).unwrap();
        assert!(!encoded.contains("private conflicting objective"));
        assert!(conflict
            .delivery
            .as_ref()
            .is_some_and(|delivery| delivery.command.is_none()));
    }

    #[tokio::test]
    async fn live_equal_submission_reports_in_progress_without_a_second_task() {
        let (_workspace, _store, host) = pending_composed_host().await;
        let request = RequestEnvelope::command(
            "pending-duplicate",
            AppCommand::SubmitTask {
                prompt: "wait once".into(),
            },
        );
        let responses = serve_requests(
            host,
            &[
                request.clone(),
                request,
                RequestEnvelope::command(
                    "pending-duplicate",
                    AppCommand::SubmitTask {
                        prompt: "private conflicting prompt".into(),
                    },
                ),
            ],
        )
        .await;
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("pending-duplicate")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. } if code == "command_in_progress"
                )
        }));
        assert_eq!(
            responses
                .iter()
                .filter(|response| matches!(
                    &response.payload,
                    ServerPayload::Event { event }
                        if matches!(event.as_ref(), AppEvent::TaskAccepted(_))
                ))
                .count(),
            1
        );
        let bindings = responses
            .iter()
            .filter(|response| response.request_id.as_deref() == Some("pending-duplicate"))
            .filter_map(|response| response.delivery.as_ref())
            .filter_map(|delivery| delivery.command.as_ref())
            .filter_map(|command| command.run_id)
            .collect::<Vec<_>>();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().all(|run_id| *run_id == bindings[0]));
        let conflict = responses
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("pending-duplicate")
                    && matches!(
                        &response.payload,
                        ServerPayload::Error { code, .. }
                            if code == "command_identity_conflict"
                    )
            })
            .expect("live conflicting duplicate");
        assert_eq!(
            conflict.delivery.as_ref().unwrap().disposition,
            CommandDeliveryDisposition::Conflict
        );
        assert!(!serde_json::to_string(conflict)
            .unwrap()
            .contains("private conflicting prompt"));
    }

    #[tokio::test]
    async fn approval_required_result_replays_without_a_new_provider_or_approval_ticket() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let requests = Arc::new(AtomicUsize::new(0));
        let mut providers = ProviderRegistry::default();
        providers.register(ApprovalRequiredProvider {
            requests: Arc::clone(&requests),
        });
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            providers,
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: argentum_domain::ProviderProfile {
                    id: "f09-approval".into(),
                    label: "F09 approval".into(),
                    kind: argentum_domain::ProviderKind::OpenAiCompatible,
                    endpoint: "https://approval.example.test/v1/".into(),
                    model: "test-model".into(),
                    selected: true,
                },
            })
            .await
            .unwrap();
        runtime
            .declare_model_budget("f09-approval", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        let request = RequestEnvelope::command(
            "approval-result",
            AppCommand::SubmitTask {
                prompt: "request one protected write".into(),
            },
        );
        let original = serve_requests(
            CommandHost::from_runtime(runtime),
            std::slice::from_ref(&request),
        )
        .await;
        let original_failure = original
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("approval-result")
                    && matches!(
                        &response.payload,
                        ServerPayload::CommandFailed {
                            code,
                            approval_id: Some(_),
                            ..
                        } if code == "approval_required"
                    )
            })
            .expect("original approval-required result");
        assert_eq!(
            original_failure.delivery.as_ref().unwrap().disposition,
            CommandDeliveryDisposition::Original
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(!workspace.path().join("must-not-run.txt").exists());

        let mut providers = ProviderRegistry::default();
        providers.register(CompletingApprovalProvider {
            requests: Arc::clone(&requests),
        });
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store,
            providers,
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("restarted runtime");
        runtime
            .declare_model_budget("f09-approval", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        let replay = serve_requests(CommandHost::from_runtime(runtime), &[request]).await;
        let replayed_failure = replay
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("approval-result")
                    && matches!(
                        &response.payload,
                        ServerPayload::CommandFailed {
                            code,
                            approval_id: None,
                            ..
                        } if code == "approval_required"
                    )
                    && response.delivery.as_ref().is_some_and(|delivery| {
                        delivery.disposition == CommandDeliveryDisposition::Replayed
                    })
            })
            .expect("replayed approval-required result");
        assert!(matches!(
            (&original_failure.payload, &replayed_failure.payload),
            (
                ServerPayload::CommandFailed {
                    code: original_code,
                    run_id: original_run,
                    tool_call_id: original_tool,
                    ..
                },
                ServerPayload::CommandFailed {
                    code: replayed_code,
                    run_id: replayed_run,
                    tool_call_id: replayed_tool,
                    approval_id: None,
                    ..
                }
            ) if original_code == replayed_code
                && original_run == replayed_run
                && original_tool == replayed_tool
        ));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(!workspace.path().join("must-not-run.txt").exists());
        assert!(replay.iter().all(|response| !matches!(
            &response.payload,
            ServerPayload::Event { event }
                if matches!(event.as_ref(), AppEvent::TaskAccepted(_))
        )));
    }

    #[tokio::test]
    async fn pruned_result_is_unavailable_without_becoming_a_cache_miss() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let command = AppCommand::PauseGoal;
        let canonical = canonical_command(&command).unwrap();
        let input = CommandClaimInput {
            project_id: runtime.project_id(),
            request_id: "pruned-command".into(),
            canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
            canonical_command: canonical.clone(),
            command_kind: "pause_goal".into(),
            scope: CommandIdentityScope {
                session_id: Some(runtime.session_id()),
                ..CommandIdentityScope::default()
            },
        };
        store.claim_command_identity(&input).unwrap();
        store
            .mark_command_executing(
                input.project_id,
                &input.request_id,
                input.canonicalization_version,
                &canonical,
            )
            .unwrap();
        store
            .settle_command_identity(
                input.project_id,
                &input.request_id,
                input.canonicalization_version,
                &canonical,
                CommandOutcomeKind::Failure,
                Some(
                    &serde_json::to_string(&ServerPayload::command_failed(
                        "definite",
                        "retained then pruned",
                        false,
                    ))
                    .unwrap(),
                ),
            )
            .unwrap();
        assert_eq!(
            store
                .prune_command_result_bodies(input.project_id, 1)
                .unwrap(),
            1
        );

        let responses = serve_requests(
            CommandHost::from_runtime(runtime),
            &[RequestEnvelope::command("pruned-command", command)],
        )
        .await;
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("pruned-command")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. } if code == "command_result_unavailable"
                )
                && response.delivery.as_ref().is_some_and(|delivery| {
                    delivery.disposition == CommandDeliveryDisposition::ResultUnavailable
                        && delivery.command.as_ref().is_some_and(|command| {
                            command.state == "settled"
                                && !command.payload_available
                                && !command.replay_eligible
                        })
                })
        }));
        assert_eq!(
            store
                .command_identity(input.project_id, "pruned-command")
                .unwrap()
                .unwrap()
                .state,
            CommandIdentityState::Settled
        );
    }

    #[tokio::test]
    async fn revoked_result_disclosure_returns_metadata_without_leaking_or_redispatching() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .expect("runtime");
        let command = AppCommand::PauseGoal;
        let canonical = canonical_command(&command).unwrap();
        let input = CommandClaimInput {
            project_id: runtime.project_id(),
            request_id: "restricted-command".into(),
            canonicalization_version: COMMAND_CANONICALIZATION_VERSION,
            canonical_command: canonical.clone(),
            command_kind: "pause_goal".into(),
            scope: CommandIdentityScope {
                session_id: Some(runtime.session_id()),
                ..CommandIdentityScope::default()
            },
        };
        store.claim_command_identity(&input).unwrap();
        store
            .settle_command_identity(
                input.project_id,
                &input.request_id,
                input.canonicalization_version,
                &canonical,
                CommandOutcomeKind::Failure,
                Some(
                    &serde_json::to_string(&ServerPayload::command_failed(
                        "private",
                        "PRIVATE_DISCLOSURE_CANARY",
                        false,
                    ))
                    .unwrap(),
                ),
            )
            .unwrap();
        store
            .restrict_command_result(input.project_id, &input.request_id, true)
            .unwrap();

        let responses = serve_requests(
            CommandHost::from_runtime(runtime),
            &[RequestEnvelope::command("restricted-command", command)],
        )
        .await;
        let encoded = serde_json::to_string(&responses).unwrap();
        assert!(!encoded.contains("PRIVATE_DISCLOSURE_CANARY"));
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("restricted-command")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. } if code == "command_result_restricted"
                )
                && response.delivery.as_ref().is_some_and(|delivery| {
                    delivery.disposition == CommandDeliveryDisposition::Restricted
                        && delivery.command.as_ref().is_some_and(|command| {
                            command.state == "settled"
                                && !command.payload_available
                                && !command.replay_eligible
                        })
                })
        }));
    }

    #[tokio::test]
    async fn read_only_ids_are_fresh_but_cannot_bypass_a_protected_binding() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let responses = serve_requests(
            host,
            &[
                RequestEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: "fresh-read".into(),
                    payload: ClientPayload::Ping,
                },
                RequestEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: "fresh-read".into(),
                    payload: ClientPayload::Ping,
                },
                RequestEnvelope::command("protected-id", AppCommand::NewSession),
                RequestEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: "protected-id".into(),
                    payload: ClientPayload::Ping,
                },
            ],
        )
        .await;
        assert_eq!(
            responses
                .iter()
                .filter(|response| {
                    response.request_id.as_deref() == Some("fresh-read")
                        && matches!(response.payload, ServerPayload::Pong)
                })
                .count(),
            2
        );
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("protected-id")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. }
                        if code == "command_identity_conflict"
                )
        }));
    }

    #[tokio::test]
    async fn cancellation_identity_stays_bound_to_its_original_target() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let original_target = argentum_domain::RunId::new_v4();
        let changed_target = argentum_domain::RunId::new_v4();
        let responses = serve_requests(
            host,
            &[
                RequestEnvelope::command(
                    "cancel-target",
                    AppCommand::CancelRun {
                        run_id: original_target,
                    },
                ),
                RequestEnvelope::command(
                    "cancel-target",
                    AppCommand::CancelRun {
                        run_id: changed_target,
                    },
                ),
            ],
        )
        .await;
        let original = responses
            .iter()
            .find(|response| {
                response.request_id.as_deref() == Some("cancel-target")
                    && response.delivery.as_ref().is_some_and(|delivery| {
                        delivery.disposition == CommandDeliveryDisposition::Original
                    })
            })
            .expect("original cancellation result");
        assert_eq!(
            original
                .delivery
                .as_ref()
                .and_then(|delivery| delivery.command.as_ref())
                .and_then(|command| command.subject_id),
            Some(original_target)
        );
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("cancel-target")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. } if code == "command_identity_conflict"
                )
                && response.delivery.as_ref().is_some_and(|delivery| {
                    delivery.disposition == CommandDeliveryDisposition::Conflict
                        && delivery.command.is_none()
                })
        }));
    }

    #[tokio::test]
    async fn command_metadata_inspection_is_read_only_and_project_scoped() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let responses = serve_requests(
            host,
            &[
                RequestEnvelope::command("created", AppCommand::NewSession),
                RequestEnvelope::inspect_command("inspect", "created"),
                RequestEnvelope::inspect_command("missing-inspect", "absent"),
            ],
        )
        .await;
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("inspect")
                && matches!(
                    &response.payload,
                    ServerPayload::CommandIdentityStatus { command }
                        if command.command_kind == "new_session"
                            && command.state == "settled"
                            && command.replay_eligible
                )
        }));
        assert!(responses.iter().any(|response| {
            response.request_id.as_deref() == Some("missing-inspect")
                && matches!(
                    &response.payload,
                    ServerPayload::Error { code, .. }
                        if code == "command_identity_not_found"
                )
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

    #[test]
    fn retained_failures_remove_credential_like_content_before_persistence() {
        let payload = bounded_terminal_payload(ServerPayload::CommandFailed {
            code: "command_failed".into(),
            message: "provider said api_key=hostile-credential-canary".into(),
            recoverable: false,
            run_id: None,
            tool_call_id: None,
            approval_id: None,
        });
        let encoded = serde_json::to_string(&payload).unwrap();
        assert!(!encoded.contains("hostile-credential-canary"));
        assert!(encoded.contains("credential-like detail was not retained"));
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
            ServerPayload::Event { event }
                if matches!(event.as_ref(), AppEvent::ProviderStatus(status)
                    if !status.connected && status.detail.contains("probe failed"))
        )));
        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event { event }
                if matches!(event.as_ref(), AppEvent::Error { message, recoverable: true }
                    if message.contains("probe failed"))
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
                        start_byte: Some(0),
                        end_byte: None,
                        start_line: None,
                        line_count: None,
                        expected_sha256: None,
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
            ServerPayload::Event { event }
                if matches!(event.as_ref(), AppEvent::ToolStarted(_) | AppEvent::ToolFinished(_))
        )));
        assert!(!responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event { event }
                if matches!(event.as_ref(), AppEvent::ApprovalRequested(_))
        )));
    }

    #[tokio::test]
    async fn protocol_rejects_phase4_authority_changes() {
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
                "revoke",
                AppCommand::RevokeApprovalGrant {
                    grant_id: approval_id,
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

        for request_id in ["write", "approve", "revoke", "provider", "profile"] {
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
        runtime
            .declare_model_budget("pending", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
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
                id: "minimax".into(),
                label: "Missing credential test".into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                endpoint: "https://api.minimax.io/v1/".into(),
                model: "synthetic-test-model".into(),
                selected: true,
            }
        };
        let profile_id = profile.id.clone();
        runtime
            .dispatch(AppCommand::SaveProviderProfile { profile })
            .await
            .expect("selected test provider");
        runtime
            .declare_model_budget(&profile_id, [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
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
        assert!(
            responses.iter().any(|response| {
                response.request_id.as_deref() == Some("run-failure")
                    && matches!(
                        &response.payload,
                        ServerPayload::CommandFailed { code, message, .. }
                            if code == "command_failed" && message.contains(expected_message)
                    )
            }),
            "unexpected terminal errors: {:?}",
            responses
                .iter()
                .filter_map(|r| match &r.payload {
                    ServerPayload::CommandFailed { code, message, .. } => Some((code, message)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        assert!(!responses.iter().any(|response| {
            response.request_id.as_deref() == Some("run-failure")
                && matches!(response.payload, ServerPayload::CommandCompleted)
        }));
        assert!(responses.iter().any(|response| matches!(
            &response.payload,
            ServerPayload::Event { event }
                if matches!(event.as_ref(), argentum_domain::AppEvent::RunStatusChanged {
                    lifecycle: argentum_domain::TaskLifecycle::Failed,
                    ..
                })
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

    async fn pending_composed_host() -> (tempfile::TempDir, Store, CommandHost) {
        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let workspace_manager = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace_manager.clone());
        let mut providers = ProviderRegistry::default();
        providers.register(PendingProvider);
        let store = Store::open_in_memory().expect("store");
        let runtime =
            RuntimeService::new_composed(store.clone(), providers, tools, workspace_manager)
                .await
                .expect("composed runtime");
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
        runtime
            .declare_model_budget("pending", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        (workspace, store, CommandHost::from_runtime(runtime))
    }

    fn pending_submit_input() -> String {
        format!(
            "{}\n",
            serde_json::to_string(&RequestEnvelope::command(
                "pending-cleanup",
                AppCommand::SubmitTask {
                    prompt: "wait".into(),
                },
            ))
            .expect("request")
        )
    }

    struct DelayedFailureReader {
        bytes: Vec<u8>,
        offset: usize,
        failure_delay: Pin<Box<tokio::time::Sleep>>,
    }

    struct PanickingWriter;

    impl AsyncWrite for PanickingWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &[u8],
        ) -> Poll<io::Result<usize>> {
            panic!("forced writer panic");
        }

        fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl DelayedFailureReader {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                offset: 0,
                failure_delay: Box::pin(tokio::time::sleep(Duration::from_millis(250))),
            }
        }

        fn poll_failure(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.failure_delay.as_mut().poll(context) {
                Poll::Ready(()) => Poll::Ready(Err(io::Error::other("forced test read failure"))),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl AsyncRead for DelayedFailureReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.offset < self.bytes.len() {
                let count = buffer.remaining().min(self.bytes.len() - self.offset);
                buffer.put_slice(&self.bytes[self.offset..self.offset + count]);
                self.offset += count;
                return Poll::Ready(Ok(()));
            }
            self.poll_failure(context)
        }
    }

    impl AsyncBufRead for DelayedFailureReader {
        fn poll_fill_buf(
            self: Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<io::Result<&[u8]>> {
            let this = self.get_mut();
            if this.offset < this.bytes.len() {
                return Poll::Ready(Ok(&this.bytes[this.offset..]));
            }
            match this.poll_failure(context) {
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => unreachable!("failure reader returned success"),
                Poll::Pending => Poll::Pending,
            }
        }

        fn consume(mut self: Pin<&mut Self>, amount: usize) {
            self.offset = (self.offset + amount).min(self.bytes.len());
        }
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
    struct CompletingProvider {
        requests: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ModelProvider for CompletingProvider {
        fn id(&self) -> &'static str {
            "f09-complete"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "F09 complete".into(),
                endpoint: "local".into(),
                connected: true,
                detail: "test".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), argentum_providers::ProviderError> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            sender
                .send(ProviderEvent::Delta("complete once".into()))
                .await
                .map_err(|_| argentum_providers::ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| argentum_providers::ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct ApprovalRequiredProvider {
        requests: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ModelProvider for ApprovalRequiredProvider {
        fn id(&self) -> &'static str {
            "f09-approval"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "F09 approval".into(),
                endpoint: "local".into(),
                connected: true,
                detail: "test".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), argentum_providers::ProviderError> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            sender
                .send(ProviderEvent::ToolCall(ModelToolCall {
                    id: "f09-write".into(),
                    name: "write_text".into(),
                    arguments: r#"{"path":"must-not-run.txt","content":"must not run"}"#.into(),
                }))
                .await
                .map_err(|_| argentum_providers::ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| argentum_providers::ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct CompletingApprovalProvider {
        requests: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ModelProvider for CompletingApprovalProvider {
        fn id(&self) -> &'static str {
            "f09-approval"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "F09 changed provider".into(),
                endpoint: "local".into(),
                connected: true,
                detail: "test".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), argentum_providers::ProviderError> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| argentum_providers::ProviderError::ConsumerClosed)
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
