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
    assert!(large
        .invoke(&handle, serde_json::json!({"message":"control"}))
        .await
        .is_err());
    large.retire().await.unwrap();
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
    assert!(session
        .invoke(&handle, serde_json::json!({"message":"control"}))
        .await
        .is_err());
    cancel.await.unwrap();
    session.retire().await.unwrap();
}
