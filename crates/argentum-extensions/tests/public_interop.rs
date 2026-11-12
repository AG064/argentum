#![cfg(windows)]
use argentum_execution::{ManagedProcessLimits, ProcessSpec};
use argentum_extensions::connection::McpSession;
use std::{collections::BTreeSet, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test]
#[ignore = "requires independently built pinned public SDK control; qualification invokes it explicitly"]
async fn unchanged_pinned_public_sdk_counter_interoperates() {
    let Some(program) = std::env::var_os("ARGENTUM_PUBLIC_MCP_CONTROL_EXE") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let spec = ProcessSpec::argv(program.to_string_lossy(), Vec::new(), root.path())
        .unwrap()
        .with_timeout(Duration::from_secs(30))
        .unwrap();
    let limits =
        ManagedProcessLimits::new(4, 95, 1024 * 1024, 512 * 1024 * 1024, 1024 * 1024 * 1024)
            .unwrap();
    let mut session = McpSession::connect_stdio(
        spec,
        limits,
        BTreeSet::from(["increment".into(), "get_value".into()]),
        CancellationToken::new(),
        || true,
    )
    .await
    .unwrap();
    session.set_resource_scope(vec!["memo://".into()]).unwrap();
    println!(
        "Actual public SDK negotiated protocol: {:?}",
        session.protocol_version()
    );
    let tools = session.discover().await.unwrap();
    let increment = &tools
        .iter()
        .find(|tool| tool.handle.name == "increment")
        .unwrap()
        .handle;
    let result = session
        .invoke(increment, serde_json::json!({}))
        .await
        .unwrap();
    assert_ne!(result["isError"], true);
    let resources = session.discover_resources().await.unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, "memo://insights");
    let memo = session.read_resource(&resources[0]).await.unwrap();
    assert!(serde_json::to_string(&memo)
        .unwrap()
        .contains("Business Intelligence Memo"));
    session.retire().await.unwrap();
}
