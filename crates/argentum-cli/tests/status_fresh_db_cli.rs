use std::path::Path;
use std::process::Command;

fn status(workspace: &Path, database: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_argentum"))
        .args(["status", "--isolated", "--workspace"])
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .args(["--json", "--no-color"])
        .env("RUST_LOG", "off")
        .output()
        .expect("status process")
}

#[test]
fn status_initializes_a_fresh_database_once_and_reuses_it() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("argentum.db");

    let first = status(&workspace, &database);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&first.stdout).expect("status json");
    assert_eq!(body["protocol_version"], 1);
    assert!(body["workspace"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert!(database.exists(), "status must create the database");

    let connection = rusqlite::Connection::open(&database).expect("database");
    let version: u16 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("schema version");
    assert_eq!(version, 22);
    let identity: String = connection
        .query_row(
            "SELECT store_id FROM runtime_store_identity WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("store identity");
    drop(connection);

    let second = status(&workspace, &database);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let reopened = rusqlite::Connection::open(&database).expect("reopened database");
    let version: u16 = reopened
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("schema version");
    assert_eq!(version, 22);
    let reopened_identity: String = reopened
        .query_row(
            "SELECT store_id FROM runtime_store_identity WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("store identity after restart");
    assert_eq!(reopened_identity, identity);
}
