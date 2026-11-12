//! Saved data from another release must not stop the app from starting.
//!
//! These start the real composed host against a database file, rewrite the
//! presentation records it saved into shapes this version cannot read, and
//! start it again. Authority records (policy, grants, sessions) are not
//! rewritten here: those stay strict and fail closed.

use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::AppCommand;

fn config(workspace: &std::path::Path, database: &std::path::Path) -> HostConfig {
    let mut config = HostConfig::in_memory(workspace);
    config.database = Some(database.to_path_buf());
    config
}

async fn start_and_stop(config: HostConfig) {
    let mut host = CommandHost::start_composed(config)
        .await
        .expect("the host starts");
    host.client()
        .dispatch(AppCommand::LoadUiPreferences)
        .await
        .expect("a command runs");
    host.shutdown().await.expect("clean shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_starts_when_saved_presentation_records_are_unreadable() {
    let workspace = tempfile::tempdir().expect("workspace");
    let data = tempfile::tempdir().expect("data");
    let database = data.path().join("argentum.db");

    start_and_stop(config(workspace.path(), &database)).await;

    let rewritten = {
        let connection = rusqlite::Connection::open(&database).expect("database");
        let layouts = connection
            .execute(
                "UPDATE layout_profiles SET payload = ?1",
                [r#"{"density":"Comfortable","visible":{"RemovedSurface":true},"widths":{},"inspector_collapsed":false,"active_inspector":"Conversation"}"#],
            )
            .expect("layout rewrite");
        connection
            .execute(
                "UPDATE ui_preferences SET payload = ?1",
                [r#"{"theme":"a theme from another release"}"#],
            )
            .expect("preferences rewrite");
        connection
            .execute("UPDATE project_setup SET payload = 'not json'", [])
            .expect("setup rewrite");
        layouts
    };
    assert!(rewritten > 0, "the first start saved a layout to rewrite");

    start_and_stop(config(workspace.path(), &database)).await;
}
