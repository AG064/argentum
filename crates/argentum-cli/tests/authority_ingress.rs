use argentum_cli::protocol::{RequestEnvelope, ResponseEnvelope, ServerPayload};
use argentum_cli::server::serve_jsonl;
use argentum_cli::{CommandHost, HostConfig, HostError};
use argentum_domain::{AppCommand, AppEvent, ApprovalScope, PlatformAuthorityState};
use argentum_runtime::RuntimeError;
use tokio::io::{AsyncReadExt, BufReader};

#[test]
fn permissions_output_projects_public_platform_authority() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database = tempfile::tempdir().expect("database directory");
    let database_path = database.path().join("argentum.db");
    let run = |json: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_argentum"));
        command
            .args(["permissions", "list", "--workspace"])
            .arg(workspace.path())
            .arg("--database")
            .arg(&database_path);
        if json {
            command.arg("--json");
        }
        command.output().expect("permissions command")
    };

    let json_output = run(true);
    assert!(
        json_output.status.success(),
        "JSON permissions failed: {}",
        String::from_utf8_lossy(&json_output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("permissions JSON");
    let authority = &value["platform_authority"];
    assert_eq!(authority["platform"], "windows");
    assert_eq!(authority["requested_tier"], "standard");
    assert_eq!(authority["active_tier"], "standard");
    assert_eq!(authority["state"], "active");
    assert_eq!(authority["generation"], 1);
    assert!(authority["probe"]["observed_identity"].is_string());
    assert!(authority["probe"]["capabilities"].is_array());

    let human_output = run(false);
    assert!(
        human_output.status.success(),
        "human permissions failed: {}",
        String::from_utf8_lossy(&human_output.stderr)
    );
    let human = String::from_utf8(human_output.stdout).expect("human permissions output");
    for label in [
        "Authority platform: windows",
        "Authority requested tier: Standard",
        "Authority active tier: Standard",
        "Authority state: Active",
        "Authority generation: 1",
        "Authority observed identity:",
        "Authority capabilities:",
    ] {
        assert!(human.contains(label), "missing {label}");
    }
    let rendered = format!("{}{}", value, human).to_ascii_lowercase();
    for forbidden in ["ticket", "proof", "issuer_id", "lease_id", "revocation"] {
        assert!(!rendered.contains(forbidden), "leaked {forbidden}");
    }
}

#[tokio::test]
async fn only_the_local_handle_can_elevate_runtime_policy() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
        .await
        .expect("host");
    let client = host.client();
    let local = host.local_handle();
    let elevate = AppCommand::SelectExecutionProfile {
        profile_id: "full-access".into(),
    };

    assert!(matches!(
        client.dispatch(elevate.clone()).await,
        Err(HostError::Runtime(RuntimeError::LocalAuthorityRequired))
    ));
    assert_eq!(
        host.harness_snapshot()
            .expect("unchanged harness")
            .selected_execution_profile_id,
        "guided"
    );

    local
        .dispatch(elevate)
        .await
        .expect("observed local elevation");
    assert_eq!(
        host.harness_snapshot()
            .expect("elevated harness")
            .selected_execution_profile_id,
        "full-access"
    );

    host.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn jsonl_authority_commands_are_rejected_without_runtime_mutation() {
    let workspace = tempfile::tempdir().expect("workspace");
    let host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
        .await
        .expect("host");
    let input = format!(
        "{}\n",
        serde_json::to_string(&RequestEnvelope::command(
            "untrusted-elevation",
            AppCommand::SelectExecutionProfile {
                profile_id: "full-access".into(),
            },
        ))
        .expect("request")
    );
    let (mut output_reader, output_writer) = tokio::io::duplex(64 * 1024);

    serve_jsonl(host, BufReader::new(input.as_bytes()), output_writer)
        .await
        .expect("JSONL server");
    let mut output = String::new();
    output_reader
        .read_to_string(&mut output)
        .await
        .expect("JSONL output");
    let responses = output
        .lines()
        .map(|line| serde_json::from_str::<ResponseEnvelope>(line).expect("response"))
        .collect::<Vec<_>>();

    assert!(responses.iter().any(|response| {
        response.request_id.as_deref() == Some("untrusted-elevation")
            && matches!(
                &response.payload,
                ServerPayload::Error {
                    code,
                    recoverable: false,
                    ..
                } if code == "command_not_permitted"
            )
    }));
    assert!(responses.iter().any(|response| {
        matches!(
            &response.payload,
            ServerPayload::Event { event }
                if matches!(
                    event.as_ref(),
                    AppEvent::HarnessSnapshotLoaded(snapshot)
                        if snapshot.selected_execution_profile_id == "guided"
                )
        )
    }));
}

#[tokio::test]
async fn command_host_alone_retires_authority_and_closes_all_handles() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
        .await
        .expect("host");
    let client = host.client();
    let local = host.local_handle();
    let mut events = host.subscribe();

    host.publish_initial_state().expect("initial state");
    let initial = receive_authority_event(&mut events).await;
    assert_eq!(
        initial,
        host.platform_authority_snapshot()
            .expect("initial authority snapshot")
    );
    assert_eq!(initial.state(), PlatformAuthorityState::Active);

    host.shutdown().await.expect("first shutdown");
    let retired = receive_authority_event(&mut events).await;
    assert_eq!(retired.state(), PlatformAuthorityState::Revoked);
    assert_eq!(
        retired,
        host.platform_authority_snapshot()
            .expect("retired authority snapshot")
    );
    host.shutdown()
        .await
        .expect("retryable idempotent shutdown");

    assert!(client.dispatch(AppCommand::NewSession).await.is_err());
    assert!(local.dispatch(AppCommand::NewSession).await.is_err());
}

#[tokio::test]
async fn local_handle_debug_and_protocol_surfaces_exclude_authority_material() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
        .await
        .expect("host");
    let local_debug = format!("{:?}", host.local_handle()).to_ascii_lowercase();
    let command_json = [
        AppCommand::NewSession,
        AppCommand::SelectExecutionProfile {
            profile_id: "full-access".into(),
        },
        AppCommand::SetHarnessCapabilityEnabled {
            capability_id: "execution.shell".into(),
            enabled: true,
        },
        AppCommand::SetOnboardingPermission {
            profile_id: "full-access".into(),
        },
        AppCommand::ApproveTool {
            approval_id: "00000000-0000-0000-0000-000000000001"
                .parse()
                .expect("approval ID"),
            scope: ApprovalScope::Once,
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, command)| {
        serde_json::to_string(&RequestEnvelope::command(
            format!("command-{index}"),
            command,
        ))
        .expect("command JSON")
    })
    .collect::<String>()
    .to_ascii_lowercase();
    let event_json = serde_json::to_string(&AppEvent::PlatformAuthorityChanged(
        host.platform_authority_snapshot()
            .expect("authority snapshot"),
    ))
    .expect("event JSON")
    .to_ascii_lowercase();

    for rendered in [&local_debug, &command_json, &event_json] {
        for forbidden in ["ticket", "proof", "issuer", "lease", "revocation"] {
            assert!(!rendered.contains(forbidden), "leaked {forbidden}");
        }
    }

    host.shutdown().await.expect("shutdown");
}

async fn receive_authority_event(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
) -> argentum_domain::PlatformAuthoritySnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let AppEvent::PlatformAuthorityChanged(snapshot) =
                events.recv().await.expect("authority event")
            {
                return snapshot;
            }
        }
    })
    .await
    .expect("bounded authority event")
}
