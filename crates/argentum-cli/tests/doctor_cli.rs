use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn executable() -> PathBuf {
    env!("CARGO_BIN_EXE_argentum").into()
}

fn run_doctor(workspace: &Path, database: &Path) -> Output {
    Command::new(executable())
        .args(["doctor", "--isolated", "--workspace"])
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .args(["--json"])
        .env("RUST_LOG", "off")
        .output()
        .expect("doctor process")
}

fn parse(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor JSON: {error}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn check_ids(report: &Value) -> Vec<&str> {
    report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|check| check["id"].as_str().expect("check id"))
        .collect()
}

#[test]
fn missing_database_emits_report_and_exits_nonzero() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("missing.db");
    let output = run_doctor(&workspace, &database);
    assert!(!output.status.success());
    let report = parse(&output);
    assert_eq!(report["command"], "doctor");
    assert_eq!(report["ok"], false);
    assert_eq!(report["healthy"], false);
    assert_eq!(report["database"]["exists"], false);
    assert_eq!(report["database"]["status"], "missing");
    assert!(check_ids(&report).contains(&"schema_current"));
    assert!(check_ids(&report).contains(&"workspace_initialized"));
}

#[test]
fn unreadable_database_is_reported_without_panicking() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("unreadable.db");
    std::fs::write(&database, b"this is not a sqlite database").expect("database");

    let output = run_doctor(&workspace, &database);
    assert!(!output.status.success());
    let report = parse(&output);
    assert_eq!(report["command"], "doctor");
    assert_eq!(report["ok"], false);
    assert!(report["database"]["schema_version"].is_null());
    assert!(matches!(
        report["database"]["status"].as_str(),
        Some("unopenable" | "unreadable" | "schema_mismatch")
    ));
}

#[test]
fn older_schema_is_a_required_failure() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("older.db");
    let connection = rusqlite::Connection::open(&database).expect("database");
    connection
        .execute_batch("PRAGMA user_version = 21;")
        .expect("schema");
    drop(connection);

    let output = run_doctor(&workspace, &database);
    assert!(!output.status.success());
    let report = parse(&output);
    assert_eq!(report["database"]["schema_version"], 21);
    assert_eq!(report["database"]["status"], "schema_mismatch");
    assert_eq!(report["ok"], false);
    assert!(report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["id"] == "schema_current"
            && check["required"] == true
            && check["ok"] == false));
}

#[test]
fn newer_schema_is_a_required_failure() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("newer.db");
    let connection = rusqlite::Connection::open(&database).expect("database");
    connection
        .execute_batch("PRAGMA user_version = 24;")
        .expect("schema");
    drop(connection);

    let output = run_doctor(&workspace, &database);
    assert!(!output.status.success());
    let report = parse(&output);
    assert_eq!(report["database"]["schema_version"], 24);
    assert_eq!(report["database"]["status"], "schema_mismatch");
    assert_eq!(report["ok"], false);
}

#[test]
fn current_schema_is_healthy_and_reports_stable_check_ids() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("current.db");

    let initialized = Command::new(executable())
        .args(["status", "--isolated", "--workspace"])
        .arg(&workspace)
        .arg("--database")
        .arg(&database)
        .args(["--json"])
        .env("RUST_LOG", "off")
        .output()
        .expect("status process");
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );

    let output = run_doctor(&workspace, &database);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = parse(&output);
    assert_eq!(report["command"], "doctor");
    assert_eq!(report["ok"], true);
    assert_eq!(report["healthy"], true);
    assert_eq!(report["database"]["schema_version"], 23);
    assert_eq!(
        check_ids(&report),
        vec![
            "workspace_exists",
            "database_exists",
            "database_openable",
            "schema_current",
            "workspace_initialized",
            "provider_ready",
            "harness_configured",
            "recovery_clear",
        ]
    );
}
