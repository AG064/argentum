#![cfg(windows)]
use argentum_execution::{ManagedProcessLimits, ProcessSpec};
use argentum_extensions::connection::McpSession;
use std::{collections::BTreeSet, time::Duration};
use tokio_util::sync::CancellationToken;

fn plan(root: &std::path::Path, mode: &str) -> ProcessSpec {
    ProcessSpec::argv(env!("CARGO_BIN_EXE_mcp_fixture"), [mode.to_owned()], root).unwrap()
}
fn limits() -> ManagedProcessLimits {
    ManagedProcessLimits::new(4, 95, 1024 * 1024, 512 * 1024 * 1024, 1024 * 1024 * 1024).unwrap()
}
#[tokio::test]
async fn unadvertised_capability_is_unavailable_not_implicitly_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = connect(dir.path(), "no-caps").await;
    assert!(matches!(
        session.discover().await,
        Err(argentum_extensions::connection::ConnectionError::Rejected(
            "server_does_not_advertise_requested_capability"
        ))
    ));
    session
        .set_resource_scope(vec!["memory://control/".into()])
        .unwrap();
    assert!(session.discover_resources().await.is_err());
    session.retire().await.unwrap();
}
async fn connect(root: &std::path::Path, mode: &str) -> McpSession {
    McpSession::connect_stdio(
        plan(root, mode),
        limits(),
        BTreeSet::from(["echo".to_owned()]),
        CancellationToken::new(),
        || true,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn real_modern_and_legacy_stdio_invocation_and_revocation() {
    let dir = tempfile::tempdir().unwrap();
    for mode in ["modern", "legacy"] {
        let mut session = connect(dir.path(), mode).await;
        let catalog = session.discover().await.unwrap();
        assert_eq!(catalog.len(), 1);
        let handle = catalog[0].handle.clone();
        let result = session
            .invoke(
                &handle,
                serde_json::json!({"message":"synthetic-current-result"}),
            )
            .await
            .unwrap();
        assert_eq!(result["content"][0]["text"], "synthetic-current-result");
        let mut foreign = handle.clone();
        foreign.connection = uuid::Uuid::new_v4();
        assert!(session
            .invoke(&foreign, serde_json::json!({}))
            .await
            .is_err());
        session.retire().await.unwrap();
        assert!(session
            .invoke(&handle, serde_json::json!({}))
            .await
            .is_err());
        let mut restarted = connect(dir.path(), mode).await;
        assert!(restarted
            .invoke(&handle, serde_json::json!({}))
            .await
            .is_err());
        restarted.retire().await.unwrap();
    }
}

#[tokio::test]
async fn malformed_schema_and_oversize_results_fail_truthfully() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = connect(dir.path(), "bad-schema").await;
    assert!(bad.discover().await.is_err());
    bad.retire().await.unwrap();
    let mut large = connect(dir.path(), "oversize").await;
    let handle = large.discover().await.unwrap()[0].handle.clone();
    assert!(matches!(
        large
            .invoke(&handle, serde_json::json!({"message":"control"}))
            .await,
        Err(argentum_extensions::connection::ConnectionError::UncertainEffect(_))
    ));
    assert!(large.discover().await.is_err());
    large.retire().await.unwrap();
}

#[tokio::test]
async fn changed_tool_definition_rejects_old_handle_before_invocation() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = connect(dir.path(), "changed-definition").await;
    let handle = session.discover().await.unwrap()[0].handle.clone();
    let result = session
        .invoke(&handle, serde_json::json!({"message":"not dispatched"}))
        .await;
    assert!(matches!(
        result,
        Err(argentum_extensions::connection::ConnectionError::Rejected(
            "tool_definition_changed_discover_again"
        ))
    ));
    assert!(
        !session.cancellation().is_cancelled(),
        "pre-dispatch stale definition is not an uncertain dispatched effect"
    );
    session.retire().await.unwrap();
    session.retire().await.unwrap();
}

#[tokio::test]
async fn resource_pages_bind_current_connection_and_serve_only_reviewed_handles() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = connect(dir.path(), "resources").await;
    session
        .set_resource_scope(vec!["memory://control/".into()])
        .unwrap();
    let first = session.discover_resources_page(None).await.unwrap();
    assert_eq!(first["resources"].as_array().unwrap().len(), 16);
    let next = session
        .discover_resources_page(first["next_cursor"].as_str())
        .await
        .unwrap();
    assert_eq!(next["resources"].as_array().unwrap().len(), 4);
    assert!(next["next_cursor"].is_null());
    let mut handle: argentum_extensions::connection::ResourceHandle =
        serde_json::from_value(first["resources"][0].clone()).unwrap();
    assert_eq!(
        session.read_resource(&handle).await.unwrap()["contents"][0]["text"],
        "CURRENT_SCOPED_RESOURCE"
    );
    handle.uri = "file:///private".into();
    assert!(session.read_resource(&handle).await.is_err());
    session
        .set_resource_scope(vec!["memory://control/".into()])
        .unwrap();
    assert!(session
        .discover_resources_page(first["next_cursor"].as_str())
        .await
        .is_err());
    session.retire().await.unwrap();
}

#[tokio::test]
async fn cancel_drains_owned_tree_and_pre_resume_denial_is_not_execution() {
    let dir = tempfile::tempdir().unwrap();
    assert!(McpSession::connect_stdio(
        plan(dir.path(), "modern"),
        limits(),
        BTreeSet::new(),
        CancellationToken::new(),
        || false
    )
    .await
    .is_err());
    let mut session = connect(dir.path(), "slow").await;
    let handle = session.discover().await.unwrap()[0].handle.clone();
    let cancellation = session.cancellation();
    let cancel = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancellation.cancel();
    });
    assert!(matches!(
        session
            .invoke(&handle, serde_json::json!({"message":"control"}))
            .await,
        Err(argentum_extensions::connection::ConnectionError::UncertainEffect(_))
    ));
    cancel.await.unwrap();
    session.retire().await.unwrap();
}
