// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Lets a paired device drive a running app: the desktop or Android window
//! keeps its own runtime, and the device becomes a second frontend for it.
//!
//! The device speaks the same JSONL protocol as `argentum serve`, so
//! `argentum device connect` works against it. Only the commands in
//! [`allowed_remotely`] run; everything that changes settings, credentials,
//! permissions or approvals stays with the owner at the device itself. A run
//! that needs approval waits for the owner there, as it would locally.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use argentum_domain::AppCommand;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;

use crate::protocol::{parse_request, ClientPayload, ResponseEnvelope, ServerPayload};
use crate::InProcessClient;

/// The largest request line a paired device may send.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const RESPONSE_QUEUE: usize = 256;
/// How long events raised by the last commands may take to reach the device.
const EVENT_DRAIN_GRACE: std::time::Duration = std::time::Duration::from_millis(250);

/// Commands a paired device may send: chat, sessions, and read-only views.
pub fn allowed_remotely(command: &AppCommand) -> bool {
    matches!(
        command,
        AppCommand::SubmitTask { .. }
            | AppCommand::CancelRun { .. }
            | AppCommand::NewSession
            | AppCommand::ForkSession { .. }
            | AppCommand::SelectSession { .. }
            | AppCommand::ListSessions { .. }
            | AppCommand::InspectSession { .. }
            | AppCommand::SetSessionTitle { .. }
            | AppCommand::SetSessionPinned { .. }
            | AppCommand::SetSessionArchived { .. }
            | AppCommand::ListProviderProfiles
            | AppCommand::ListToolCatalog
            | AppCommand::ListSkillCatalog
            | AppCommand::ListHarnessState
            | AppCommand::LoadAccounting
            | AppCommand::LoadRunDiagnostics { .. }
            | AppCommand::LoadRunResult { .. }
            | AppCommand::LoadContextSnapshot
            | AppCommand::LoadTrajectory { .. }
            | AppCommand::LoadChangeSet { .. }
            | AppCommand::LoadUnifiedDiff { .. }
            | AppCommand::LoadVerificationEvidence { .. }
            | AppCommand::LoadProviderUsage
            | AppCommand::RefreshGitStatus
            | AppCommand::LoadGitFileDiff { .. }
    )
}

/// Serves one paired device until it hangs up.
pub async fn serve_remote_client<R, W>(
    client: InProcessClient,
    workspace: PathBuf,
    reader: R,
    writer: W,
) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let sequence = Arc::new(AtomicU64::new(0));
    let (responses, mut queue) = mpsc::channel::<(Option<String>, ServerPayload)>(RESPONSE_QUEUE);
    let writer_sequence = Arc::clone(&sequence);
    let writing = tokio::spawn(async move {
        let mut writer = writer;
        while let Some((request_id, payload)) = queue.recv().await {
            let number = writer_sequence.fetch_add(1, Ordering::SeqCst) + 1;
            let mut line = serde_json::to_vec(&ResponseEnvelope::new(number, request_id, payload))
                .map_err(std::io::Error::other)?;
            line.push(b'\n');
            writer.write_all(&line).await?;
            writer.flush().await?;
        }
        Ok::<(), std::io::Error>(())
    });

    let _ = responses
        .send((
            None,
            ServerPayload::Ready {
                workspace: crate::server::display_workspace(&workspace),
            },
        ))
        .await;

    let mut events = client.subscribe();
    let event_responses = responses.clone();
    let forwarding = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if event_responses
                        .send((None, ServerPayload::event(event)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let mut commands = JoinSet::new();
    let result = read_requests(&client, reader, &responses, &mut commands).await;
    // A device that sends its requests and then stops writing still gets their
    // results and the events they raise, so finish them before closing.
    while commands.join_next().await.is_some() {}
    tokio::time::sleep(EVENT_DRAIN_GRACE).await;
    forwarding.abort();
    drop(responses);
    let written = writing.await.map_err(std::io::Error::other)?;
    result.and(written)
}

async fn read_requests<R>(
    client: &InProcessClient,
    mut reader: R,
    responses: &mpsc::Sender<(Option<String>, ServerPayload)>,
    commands: &mut JoinSet<()>,
) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
{
    loop {
        let mut line = Vec::new();
        let read = (&mut reader)
            .take(MAX_REQUEST_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)
            .await?;
        if read == 0 {
            return Ok(());
        }
        if line.len() > MAX_REQUEST_BYTES {
            let _ = responses
                .send((
                    None,
                    ServerPayload::error(
                        "request_too_large",
                        "request is larger than 1 MiB",
                        false,
                    ),
                ))
                .await;
            return Ok(());
        }
        let request = match parse_request(&line) {
            Ok(request) => request,
            Err(message) => {
                let _ = responses
                    .send((
                        None,
                        ServerPayload::error("malformed_request", message, true),
                    ))
                    .await;
                continue;
            }
        };
        let id = Some(request.request_id.clone());
        match request.payload {
            ClientPayload::Ping => {
                let _ = responses.send((id, ServerPayload::Pong)).await;
            }
            ClientPayload::InspectCommand { .. } => {
                let _ = responses
                    .send((
                        id,
                        ServerPayload::error(
                            "unsupported",
                            "a paired device cannot inspect command identities",
                            true,
                        ),
                    ))
                    .await;
            }
            ClientPayload::Command { command } if !allowed_remotely(&command) => {
                let _ = responses
                    .send((
                        id,
                        ServerPayload::command_failed(
                            "not_allowed_remotely",
                            "this command can only be used on the device itself",
                            false,
                        ),
                    ))
                    .await;
            }
            ClientPayload::Command { command } => {
                let _ = responses
                    .send((id.clone(), ServerPayload::CommandAccepted))
                    .await;
                // A task runs for a while; keep reading so the device can cancel it.
                let client = client.clone();
                let responses = responses.clone();
                commands.spawn(async move {
                    let outcome = match client.dispatch(command).await {
                        Ok(()) => ServerPayload::CommandCompleted,
                        Err(error) => ServerPayload::host_command_failed(&error),
                    };
                    let _ = responses.send((id, outcome)).await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RequestEnvelope;
    use crate::{CommandHost, HostConfig};

    fn line(request_id: &str, command: AppCommand) -> String {
        format!(
            "{}\n",
            serde_json::to_string(&RequestEnvelope::command(request_id, command)).unwrap()
        )
    }

    #[test]
    fn settings_credentials_and_approvals_stay_on_the_device() {
        assert!(allowed_remotely(&AppCommand::SubmitTask {
            prompt: "hi".into()
        }));
        assert!(!allowed_remotely(&AppCommand::ListApprovalGrants));
        assert!(!allowed_remotely(&AppCommand::ResetLayout));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_paired_device_lists_sessions_and_is_refused_a_settings_change() {
        let workspace = tempfile::tempdir().unwrap();
        let host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .unwrap();
        let (device, app) = tokio::io::duplex(1024 * 1024);
        let (app_read, app_write) = tokio::io::split(app);
        let (device_read, mut device_write) = tokio::io::split(device);
        let serving = tokio::spawn(serve_remote_client(
            host.client(),
            workspace.path().to_path_buf(),
            tokio::io::BufReader::new(app_read),
            app_write,
        ));

        device_write
            .write_all(
                line(
                    "list",
                    AppCommand::ListSessions {
                        query: String::new(),
                        filter: Default::default(),
                        limit: 20,
                        cursor: None,
                    },
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        device_write
            .write_all(line("layout", AppCommand::ResetLayout).as_bytes())
            .await
            .unwrap();

        // Disconnect at once, as `argentum device connect` does with piped input.
        device_write.shutdown().await.unwrap();
        let mut lines = tokio::io::BufReader::new(device_read).lines();
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        while let Some(next) = tokio::time::timeout_at(deadline, lines.next_line())
            .await
            .expect("responses in time")
            .unwrap()
        {
            seen.push(serde_json::from_str::<serde_json::Value>(&next).unwrap());
        }
        assert!(seen
            .iter()
            .any(|v| v["request_id"] == "list" && v["type"] == "command_completed"));
        assert_eq!(seen[0]["type"], "ready");
        let refused = seen
            .iter()
            .find(|v| v["request_id"] == "layout" && v["type"] == "command_failed")
            .unwrap();
        assert_eq!(refused["code"], "not_allowed_remotely");
        assert!(seen
            .iter()
            .any(|v| !v["event"]["SessionListLoaded"].is_null()));

        tokio::time::timeout(std::time::Duration::from_secs(10), serving)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(host);
    }
}
