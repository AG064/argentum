use std::path::Path;
use std::process::{Command, Output};

fn run_cli(workspace: &Path, database: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_argentum"))
        .args(arguments)
        .arg("--workspace")
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .arg("--json")
        .output()
        .expect("run argentum")
}

fn json(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn phase4_cli_lists_tools_profiles_grants_and_verification_evidence() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database_dir = tempfile::tempdir().expect("database directory");
    let database = database_dir.path().join("argentum.db");

    let tools = json(run_cli(workspace.path(), &database, &["tool", "list"]));
    let tools = tools["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 22);
    for id in [
        "start_verification",
        "poll_process_job",
        "cancel_process_job",
    ] {
        assert!(tools.iter().any(|tool| tool["id"] == id));
    }
    assert!(tools
        .iter()
        .any(|tool| tool["id"] == "execute_argv" && tool["enabled"] == true));
    assert!(tools
        .iter()
        .any(|tool| tool["id"] == "delegate_readonly" && tool["enabled"] == false));
    assert!(!tools
        .iter()
        .any(|tool| matches!(tool["id"].as_str(), Some("network" | "terminal"))));

    let guided = json(run_cli(
        workspace.path(),
        &database,
        &["permissions", "list"],
    ));
    assert_eq!(guided["selected"]["profile_id"], "guided");
    assert_eq!(guided["selected"]["sandbox_scope"], "workspace_write");
    assert_eq!(guided["selected"]["approval_mode"], "always");

    let full_access = run_cli(
        workspace.path(),
        &database,
        &["permissions", "select", "full-access"],
    );
    assert!(!full_access.status.success());
    assert!(full_access.stdout.is_empty());
    assert_eq!(
        String::from_utf8(full_access.stderr)
            .expect("UTF-8 error output")
            .trim(),
        "argentum: local authority is required for this command"
    );

    let still_guided = json(run_cli(
        workspace.path(),
        &database,
        &["permissions", "list"],
    ));
    assert_eq!(still_guided["selected"]["profile_id"], "guided");
    assert_eq!(still_guided["selected"]["sandbox_scope"], "workspace_write");
    assert_eq!(still_guided["selected"]["approval_mode"], "always");

    let grants = json(run_cli(
        workspace.path(),
        &database,
        &["permissions", "grants"],
    ));
    assert_eq!(grants["grants"].as_array().expect("grants").len(), 0);

    let runners = json(run_cli(
        workspace.path(),
        &database,
        &["verification", "list"],
    ));
    assert_eq!(runners["runners"].as_array().expect("runners").len(), 4);

    let evidence = json(run_cli(
        workspace.path(),
        &database,
        &["verification", "status"],
    ));
    assert!(evidence["current_revision"].as_str().is_some());
    assert!(evidence["evidence"]
        .as_array()
        .expect("evidence")
        .is_empty());
}
