use std::path::Path;
use std::process::{Command, Output};

fn run_cli(workspace: &Path, database: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_argentum-cli"))
        .args(arguments)
        .arg("--workspace")
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .arg("--json")
        .output()
        .expect("run argentum-cli")
}

fn harness_snapshot(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn harness_presentation_and_execution_policy_persist_without_enabling_missing_work() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database_dir = tempfile::tempdir().expect("database directory");
    let database = database_dir.path().join("argentum.db");

    let selected = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "profile", "review"],
    ));
    assert_eq!(selected["harness"]["selected_profile_id"], "review");

    let restarted = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "status"],
    ));
    assert_eq!(restarted["harness"]["selected_profile_id"], "review");
    assert!(restarted["harness"]["surfaces"]
        .as_array()
        .expect("surfaces")
        .iter()
        .any(|surface| surface["id"] == "Changes" && surface["visible"] == true));

    let custom = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "surface", "activity", "show"],
    ));
    assert_eq!(custom["harness"]["selected_profile_id"], "custom");
    assert!(custom["harness"]["surfaces"]
        .as_array()
        .expect("surfaces")
        .iter()
        .any(|surface| surface["id"] == "Activity" && surface["visible"] == true));

    let unavailable = run_cli(
        workspace.path(),
        &database,
        &["harness", "surface", "terminal", "show"],
    );
    assert!(!unavailable.status.success());
    let stderr = String::from_utf8_lossy(&unavailable.stderr);
    assert!(stderr.contains("not available in this build"));

    let after_failure = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "status"],
    ));
    assert_eq!(after_failure["harness"]["selected_profile_id"], "custom");
    assert!(after_failure["harness"]["surfaces"]
        .as_array()
        .expect("surfaces")
        .iter()
        .any(|surface| {
            surface["id"] == "Terminal"
                && surface["availability"] == "unavailable"
                && surface["visible"] == false
        }));

    let read_only = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "execution", "read-only"],
    ));
    assert_eq!(
        read_only["harness"]["selected_execution_profile_id"],
        "read-only"
    );
    assert!(read_only["harness"]["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(|capability| {
            capability["id"] == "tool.write-text"
                && capability["availability"] == "available"
                && capability["enabled"] == false
                && capability["configurable"] == true
        }));

    let restarted_read_only = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "status"],
    ));
    assert_eq!(
        restarted_read_only["harness"]["selected_execution_profile_id"],
        "read-only"
    );

    let custom_execution = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "capability", "tool.write-text", "enable"],
    ));
    assert_eq!(
        custom_execution["harness"]["selected_execution_profile_id"],
        "custom"
    );
    assert!(custom_execution["harness"]["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(|capability| {
            capability["id"] == "tool.write-text" && capability["enabled"] == true
        }));

    let unavailable_capability = run_cli(
        workspace.path(),
        &database,
        &["harness", "capability", "verification.runner", "enable"],
    );
    assert!(!unavailable_capability.status.success());
    assert!(String::from_utf8_lossy(&unavailable_capability.stderr)
        .contains("not available in this build"));

    let trace = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "profile", "trace"],
    ));
    assert!(trace["harness"]["surfaces"]
        .as_array()
        .expect("surfaces")
        .iter()
        .any(|surface| surface["id"] == "Trajectory" && surface["visible"] == true));
    assert!(trace["harness"]["surfaces"]
        .as_array()
        .expect("surfaces")
        .iter()
        .any(|surface| surface["id"] == "Activity" && surface["visible"] == false));

    let trajectory = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "trajectory"],
    ));
    assert!(trajectory["trajectory"]["session_id"].is_string());
    assert!(trajectory["trajectory"]["entries"].is_array());
    assert_eq!(trajectory["trajectory"]["truncated"], false);
}
