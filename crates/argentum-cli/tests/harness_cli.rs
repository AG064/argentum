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

fn harness_snapshot(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

fn assert_local_authority_required(output: Output) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr)
            .expect("UTF-8 error output")
            .trim(),
        "argentum: local authority is required for this command"
    );
}

#[test]
fn isolated_cli_adds_presets_without_overwriting_and_controls_optional_capture() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let database = data.path().join("state.db");
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_argentum"))
            .args(arguments)
            .arg("--isolated")
            .arg("--workspace")
            .arg(workspace.path())
            .arg("--database")
            .arg(&database)
            .arg("--json")
            .output()
            .unwrap()
    };
    let first = harness_snapshot(&run(&["provider", "add", "deepseek"]));
    assert!(first
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "deepseek" && p["selected"] == true));
    harness_snapshot(&run(&[
        "provider",
        "save",
        "deepseek",
        "--label",
        "Saved profile",
        "--kind",
        "openai-compatible",
        "--endpoint",
        "https://api.deepseek.com/v1/",
        "--model",
        "saved-model",
        "--select",
    ]));
    let repeated = harness_snapshot(&run(&["provider", "add", "deepseek"]));
    let selected = repeated
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["selected"] == true)
        .unwrap();
    assert_eq!(selected["model"], "saved-model");
    assert_eq!(selected["label"], "Saved profile");
    assert!(!run(&["provider", "add", "unsupported"]).status.success());
    let initial = harness_snapshot(&run(&["harness", "status"]));
    assert!(initial["harness"].is_object());
    assert!(initial["harness"]["local_backend"].is_null());
    assert!(initial["harness"]["diagnostics"].is_null());
    harness_snapshot(&run(&[
        "harness",
        "module",
        "first-party.diagnostics",
        "enable",
    ]));
    let restarted = harness_snapshot(&run(&["harness", "status"]));
    assert!(restarted["harness"]["diagnostics"]["records"]
        .as_array()
        .unwrap()
        .is_empty());
    harness_snapshot(&run(&[
        "harness",
        "module",
        "first-party.diagnostics",
        "remove",
    ]));
    assert!(harness_snapshot(&run(&["harness", "status"]))["harness"]["diagnostics"].is_null());
    assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
    assert!(database.is_file());
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

    let disabled_surface = run_cli(
        workspace.path(),
        &database,
        &["harness", "surface", "activity", "show"],
    );
    assert!(!disabled_surface.status.success());
    let enabled_module = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "module", "first-party.activity", "enable"],
    ));
    assert!(enabled_module["harness"]["modules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "first-party.activity" && m["enabled"] == true));

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

    let without_verification = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "capability", "verification.runner", "disable"],
    ));
    assert_eq!(
        without_verification["harness"]["selected_execution_profile_id"],
        "custom"
    );
    assert!(without_verification["harness"]["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(|capability| {
            capability["id"] == "verification.runner"
                && capability["availability"] == "available"
                && capability["enabled"] == false
                && capability["configurable"] == true
        }));
    assert_local_authority_required(run_cli(
        workspace.path(),
        &database,
        &["harness", "capability", "verification.runner", "enable"],
    ));

    let read_only = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "execution", "read-only"],
    ));
    assert_eq!(
        read_only["harness"]["selected_execution_profile_id"],
        "observe"
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
        "observe"
    );

    for capability_id in ["tool.write-text", "execution.argv"] {
        assert_local_authority_required(run_cli(
            workspace.path(),
            &database,
            &["harness", "capability", capability_id, "enable"],
        ));
    }

    let after_rejected_enables = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "status"],
    ));
    assert_eq!(
        after_rejected_enables["harness"]["selected_execution_profile_id"],
        "observe"
    );
    for capability_id in ["tool.write-text", "execution.argv", "verification.runner"] {
        assert!(after_rejected_enables["harness"]["capabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|capability| {
                capability["id"] == capability_id
                    && capability["availability"] == "available"
                    && capability["enabled"] == false
                    && capability["configurable"] == true
            }));
    }

    harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "module", "first-party.trajectory", "enable"],
    ));
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

    harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "module", "first-party.activity", "remove"],
    ));
    let removed = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "status"],
    ));
    assert!(removed["harness"]["modules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "first-party.activity"
            && m["installed"] == false
            && m["enabled"] == false));
    let installed = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "module", "first-party.activity", "install"],
    ));
    assert!(installed["harness"]["modules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "first-party.activity"
            && m["installed"] == true
            && m["enabled"] == false));
    assert_eq!(
        installed["harness"]["selected_execution_profile_id"],
        "observe"
    );
}

#[cfg(windows)]
#[test]
fn backend_choice_persists_across_restarts_and_other_module_changes() {
    let workspace = tempfile::tempdir().unwrap();
    let database_dir = tempfile::tempdir().unwrap();
    let database = database_dir.path().join("modules.db");
    let disabled = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "backend", "disable"],
    ));
    assert_eq!(disabled["harness"]["local_backend"]["enabled"], false);
    harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "module", "first-party.activity", "enable"],
    ));
    let restarted = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "status"],
    ));
    assert_eq!(restarted["harness"]["local_backend"]["enabled"], false);
    let enabled = harness_snapshot(&run_cli(
        workspace.path(),
        &database,
        &["harness", "backend", "enable"],
    ));
    assert_eq!(enabled["harness"]["local_backend"]["enabled"], true);
}
