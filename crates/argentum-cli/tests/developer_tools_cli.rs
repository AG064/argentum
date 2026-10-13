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
        .arg("--isolated")
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
fn fixed_verification_survives_generic_argv_disable_and_fresh_process_readback() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database_dir = tempfile::tempdir().expect("database directory");
    let database = database_dir.path().join("argentum.db");
    // Supported trusted owner API only in this disposable test store. Production
    // activation and project-runner approval are not simulated by this fixture.
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut config = argentum_cli::HostConfig::in_memory(workspace.path());
        config.database = Some(database.clone());
        let mut host = argentum_cli::CommandHost::start(config).unwrap();
        let owner = host.local_handle();
        owner
            .dispatch(argentum_domain::AppCommand::SelectExecutionProfile {
                profile_id: "verification".into(),
            })
            .await
            .unwrap();
        // Emulate a retained policy from before the obsolete dependency repair.
        owner
            .dispatch(argentum_domain::AppCommand::SetHarnessCapabilityEnabled {
                capability_id: "execution.argv".into(),
                enabled: true,
            })
            .await
            .unwrap();
        host.shutdown().await.unwrap();
    });
    // The actual CLI command that previously failed, in a new process.
    json(run_cli(
        workspace.path(),
        &database,
        &["harness", "capability", "execution.argv", "disable"],
    ));
    let policy = json(run_cli(
        workspace.path(),
        &database,
        &["permissions", "list"],
    ));
    assert_eq!(policy["selected"]["profile_id"], "custom");
    assert_eq!(
        policy["selected"]["capability_enabled"]["execution.argv"],
        false
    );
    assert_eq!(
        policy["selected"]["capability_enabled"]["verification.runner"],
        true
    );
    let tools = json(run_cli(workspace.path(), &database, &["tool", "list"]));
    let tools = tools["tools"].as_array().unwrap();
    for id in [
        "verify",
        "start_verification",
        "poll_process_job",
        "cancel_process_job",
    ] {
        assert!(tools
            .iter()
            .any(|tool| tool["id"] == id && tool["enabled"] == true));
    }
    for id in ["execute_argv", "execute_shell"] {
        assert!(tools
            .iter()
            .any(|tool| tool["id"] == id && tool["enabled"] == false));
    }
    let retained = [
        "extension.external",
        "verification.runner",
        "tool.read-text",
    ];
    let capabilities = tools
        .iter()
        .map(|tool| tool["capability_id"].as_str().unwrap())
        .filter(|id| !retained.contains(id))
        .collect::<std::collections::BTreeSet<_>>();
    // Match the owner script's restriction sequence through fresh CLI processes.
    for id in capabilities {
        json(run_cli(
            workspace.path(),
            &database,
            &["harness", "capability", id, "disable"],
        ));
    }
    let restricted = json(run_cli(workspace.path(), &database, &["tool", "list"]));
    let restricted = restricted["tools"].as_array().unwrap();
    assert!(restricted
        .iter()
        .filter(|tool| tool["enabled"] == true)
        .all(|tool| { retained.contains(&tool["capability_id"].as_str().unwrap()) }));
    for id in ["verify", "read_text"] {
        assert!(restricted
            .iter()
            .any(|tool| tool["id"] == id && tool["enabled"] == true));
    }
    let grants = json(run_cli(
        workspace.path(),
        &database,
        &["permissions", "grants"],
    ));
    assert!(grants["grants"].as_array().unwrap().is_empty());
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut config = argentum_cli::HostConfig::in_memory(workspace.path());
        config.database = Some(database.clone());
        let mut host = argentum_cli::CommandHost::start(config).unwrap();
        assert_eq!(
            host.execution_policy()
                .unwrap()
                .source_profile_id
                .as_deref(),
            Some("verification")
        );
        host.shutdown().await.unwrap();
    });
}

#[test]
fn phase4_cli_lists_tools_profiles_grants_and_verification_evidence() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database_dir = tempfile::tempdir().expect("database directory");
    let database = database_dir.path().join("argentum.db");

    let tools = json(run_cli(workspace.path(), &database, &["tool", "list"]));
    let tools = tools["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 23 + usize::from(cfg!(feature = "extensions")));
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
    assert!(tools
        .iter()
        .any(|tool| tool["id"] == "check_rust" && tool["enabled"] == false));
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
