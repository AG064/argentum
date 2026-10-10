use std::time::Duration;

use argentum_cli::{CommandHost, HostConfig, HostError};
use argentum_domain::AppCommand;
use argentum_runtime::RuntimeError;
use tokio::sync::broadcast::error::RecvError;

const SHUTDOWN_TEST_TIMEOUT: Duration = Duration::from_secs(2);

#[tokio::test]
async fn sole_owner_shutdown_contract_closes_admission_and_runtime_state() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut owner = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
        .await
        .expect("host owner");
    let untrusted = owner.client();
    let local = owner.local_handle();
    let observer = owner.lifecycle_observer();
    let mut events = owner.subscribe();

    owner.request_shutdown().expect("first shutdown request");
    owner.request_shutdown().expect("repeated shutdown request");

    assert!(matches!(
        untrusted.dispatch(AppCommand::NewSession).await,
        Err(HostError::Runtime(RuntimeError::ShuttingDown))
    ));
    assert!(matches!(
        local.dispatch(AppCommand::NewSession).await,
        Err(HostError::Runtime(RuntimeError::ShuttingDown))
    ));

    owner.shutdown().await.expect("clean shutdown");
    let lifecycle = observer
        .harness_lifecycle_snapshot()
        .await
        .expect("composed lifecycle");
    assert!(
        lifecycle.module_ids().is_empty(),
        "shutdown returned before lifecycle retirement completed"
    );

    owner
        .request_shutdown()
        .expect("post-shutdown request remains idempotent");
    owner
        .shutdown()
        .await
        .expect("post-shutdown cleanup remains idempotent");

    drop(owner);
    drop(untrusted);
    drop(local);
    drop(observer);

    tokio::time::timeout(SHUTDOWN_TEST_TIMEOUT, async {
        while let Ok(_) | Err(RecvError::Lagged(_)) = events.recv().await {}
    })
    .await
    .expect("event broadcast closes after every sender is dropped");
}
