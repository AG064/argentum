// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Error codes, messages and confirmations for the CLI polish fixes. Every
//! command runs isolated, with its own data directory, workspace and database.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const MISSING_ID: &str = "11111111-1111-4111-8111-111111111111";

struct Sandbox {
    _directory: tempfile::TempDir,
    data: PathBuf,
    workspace: PathBuf,
    database: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("directory");
        let data = directory.path().join("data");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&data).expect("data");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        Self {
            _directory: directory,
            data,
            workspace,
            database,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_argentum"));
        command
            .args(args)
            .args(["--isolated", "--workspace"])
            .arg(&self.workspace)
            .arg("--database")
            .arg(&self.database)
            .env("ARGENTUM_DATA_DIR", &self.data)
            .env("RUST_LOG", "off")
            .stdin(Stdio::null());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().expect("process")
    }

    fn json(&self, args: &[&str]) -> (Output, serde_json::Value) {
        let mut all = args.to_vec();
        all.push("--json");
        let output = self.run(&all);
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("not JSON ({error}): {text}"));
        (output, value)
    }

    /// Error envelope code for `schema_version` errors or machine responses.
    fn failure_code(&self, args: &[&str]) -> String {
        let (output, value) = self.json(args);
        assert!(!output.status.success(), "{args:?} must fail: {value}");
        value["error"]["code"]
            .as_str()
            .or_else(|| value["code"].as_str())
            .unwrap_or_else(|| panic!("no code in {value}"))
            .to_owned()
    }

    fn init(&self) {
        let output = self.run(&["status"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn missing_records_are_not_found() {
    let sandbox = Sandbox::new();
    sandbox.init();
    for args in [
        vec!["session", "inspect", MISSING_ID],
        vec!["changes", "page", "--run-id", MISSING_ID],
        vec!["changes", "list", MISSING_ID],
        vec!["changes", "diff", MISSING_ID],
        vec!["changes", "restore", MISSING_ID],
        vec!["provider", "select", "no-such-profile"],
        vec!["provider", "model", "no-such-profile", "--model", "m"],
        vec!["skills", "inspect", "no-such-skill"],
    ] {
        assert_eq!(sandbox.failure_code(&args), "not_found", "{args:?}");
    }
    let (_, value) = sandbox.json(&["session", "inspect", MISSING_ID]);
    let message = value["error"]["message"].as_str().expect("message");
    assert!(message.contains("session not found"), "{message}");
    assert!(!message.contains("stored record is invalid"), "{message}");
    let (_, value) = sandbox.json(&["provider", "select", "no-such-profile"]);
    let message = value["error"]["message"].as_str().expect("message");
    assert!(message.contains("provider profile not found"), "{message}");
}

#[test]
fn bad_input_is_invalid_argument() {
    let sandbox = Sandbox::new();
    sandbox.init();
    for args in [
        vec!["result", MISSING_ID],
        vec![
            "goal",
            "criteria",
            "--goal-id",
            MISSING_ID,
            "--runners",
            "not-a-runner",
        ],
        vec![
            "provider",
            "save",
            "custom",
            "--label",
            "Custom",
            "--kind",
            "openai-compatible",
            "--endpoint",
            "https://user:secret@example.com/v1",
            "--model",
            "m",
        ],
        vec!["permissions", "select", "no-such-profile"],
        vec!["verification", "approve", "cargo-check-locked"],
        vec!["provider", "credential", "clear", "minimax"],
    ] {
        assert_eq!(sandbox.failure_code(&args), "invalid_argument", "{args:?}");
    }
}

#[test]
fn credential_flag_messages_never_echo_url_credentials() {
    let sandbox = Sandbox::new();
    sandbox.init();
    let (_, value) = sandbox.json(&[
        "provider",
        "save",
        "custom",
        "--label",
        "Custom",
        "--kind",
        "openai-compatible",
        "--endpoint",
        "https://user:secret@example.com/v1",
        "--model",
        "m",
    ]);
    assert!(!value.to_string().contains("secret"), "{value}");
}

#[test]
fn fixed_runner_approval_explains_that_none_is_needed() {
    let sandbox = Sandbox::new();
    sandbox.init();
    let (_, value) = sandbox.json(&["verification", "approve", "cargo-check-locked"]);
    let message = value["error"]["message"].as_str().expect("message");
    assert!(message.contains("fixed runner"), "{message}");
    assert!(message.contains("needs no approval"), "{message}");
    assert!(!message.contains("project runner unavailable"), "{message}");
}

#[test]
fn clearing_a_credential_needs_yes_outside_a_terminal() {
    let sandbox = Sandbox::new();
    sandbox.init();
    let output = sandbox.run(&["provider", "credential", "clear", "minimax"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("--yes"), "{error}");
    // With --yes it proceeds. The isolated build has no secret store, so it must
    // fail on storage and never claim that a credential was removed.
    let output = sandbox.run(&["provider", "credential", "clear", "minimax", "--yes"]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("Credential removed"), "{text}");
    // --yes belongs to clear only.
    let output = sandbox.run(&["provider", "credential", "set", "minimax", "--yes"]);
    assert!(!output.status.success());
}

#[test]
fn zero_change_runs_list_as_empty() {
    let sandbox = Sandbox::new();
    sandbox.init();
    let run_id = "22222222-2222-4222-8222-222222222222";
    {
        let connection = rusqlite::Connection::open(&sandbox.database).expect("database");
        let project: String = connection
            .query_row("SELECT id FROM projects LIMIT 1", [], |row| row.get(0))
            .expect("project");
        let session: String = connection
            .query_row("SELECT id FROM sessions LIMIT 1", [], |row| row.get(0))
            .expect("session");
        connection
            .execute(
                "INSERT INTO event_log (event_type, created_at, payload, project_id, session_id, run_id)
                 VALUES ('run_status_changed', 1, ?4, ?1, ?2, ?3)",
                rusqlite::params![
                    project,
                    session,
                    run_id,
                    format!(
                        "{{\"RunStatusChanged\":{{\"run_id\":\"{run_id}\",\"lifecycle\":\"Complete\"}}}}"
                    )
                ],
            )
            .expect("run event");
    }
    let (output, page) = sandbox.json(&["changes", "page", "--run-id", run_id]);
    assert!(output.status.success(), "{page}");
    assert_eq!(page["page"]["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(page["page"]["complete"], true);
    let (output, list) = sandbox.json(&["changes", "list", run_id]);
    assert!(output.status.success(), "{list}");
    let events = list["events"].as_array().expect("events");
    assert_eq!(events.len(), 1, "{list}");
    let snapshot = &events[0]["ChangeSetSnapshotLoaded"];
    assert_eq!(
        snapshot["files"].as_array().map(Vec::len),
        Some(0),
        "{list}"
    );
    assert_eq!(snapshot["run_id"], run_id);
}

#[cfg(not(feature = "desktop-control"))]
#[test]
fn desktop_status_matches_the_other_desktop_commands() {
    let sandbox = Sandbox::new();
    sandbox.init();
    assert_eq!(
        sandbox.failure_code(&["desktop", "status"]),
        "feature_unavailable"
    );
    assert_eq!(
        sandbox.failure_code(&["desktop", "list"]),
        "feature_unavailable"
    );
}

#[cfg(not(feature = "extensions"))]
#[test]
fn browser_status_prints_json_without_extensions() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["extensions", "browser", "status"]);
    assert!(!output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON without --json");
    assert_eq!(value["ok"], false);
    assert_eq!(value["command"], "extensions browser status");
    assert_eq!(value["error"]["code"], "feature_unavailable");
}

#[test]
fn hosted_profiles_do_not_claim_local_coding_configuration() {
    let sandbox = Sandbox::new();
    sandbox.init();
    let output = sandbox.run(&["provider", "add", "minimax"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("Local coding configuration ready"), "{text}");
    assert!(
        text.contains("Provider ready") || text.contains("Coding setup incomplete"),
        "{text}"
    );
}

#[test]
fn attestation_is_rejected_when_the_goal_does_not_require_it() {
    let sandbox = Sandbox::new();
    sandbox.init();
    let (output, set) = sandbox.json(&["goal", "set", "Ship the thing"]);
    assert!(output.status.success(), "{set}");
    let goal_id = set["goal"]["id"].as_str().expect("goal id").to_owned();
    // Timestamps are RFC 3339 strings in CLI JSON.
    for text in [
        &set["goal"]["created_at"],
        &set["goal"]["accounting"]["started_at"],
    ] {
        let text = text
            .as_str()
            .unwrap_or_else(|| panic!("not a string: {set}"));
        assert!(text.contains('T') && text.ends_with('Z'), "{text}");
    }
    let (output, criteria) = sandbox.json(&[
        "goal",
        "criteria",
        "--goal-id",
        &goal_id,
        "--runners",
        "cargo-check-locked",
    ]);
    assert!(output.status.success(), "{criteria}");
    let (output, attest) =
        sandbox.json(&["goal", "attest", "--goal-id", &goal_id, "--summary", "done"]);
    assert!(!output.status.success(), "{attest}");
    assert_eq!(attest["error"]["code"], "invalid_argument");
    assert!(
        attest["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("does not require owner attestation")),
        "{attest}"
    );
}

#[test]
fn serve_names_unknown_fields_invalid_request_and_garbage_invalid_json() {
    let sandbox = Sandbox::new();
    let mut child = sandbox
        .command(&["serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("serve");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(
                b"{\"protocol_version\":1,\"request_id\":\"a\",\"type\":\"ping\",\"bogus\":1}\n",
            )
            .expect("write");
        stdin.write_all(b"this is not json\n").expect("write");
    }
    let output = child.wait_with_output().expect("serve output");
    let text = String::from_utf8_lossy(&output.stdout);
    let codes = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["type"] == "error")
        .filter_map(|value| value["code"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(codes, ["invalid_request", "invalid_json"], "{text}");
}
