//! F08 process-loss recovery through exact CLI executables and loopback HTTP.

use super::*;
use argentum_domain::recovery::{
    OwnershipOutcome, RecoveryAccountingState, RecoveryExecutionState, RecoveryRequestState,
};
use argentum_domain::RunId;
use argentum_store::{Store, StoreError};
use std::path::Path;

struct RecoveryServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
    first_boundary: mpsc::Receiver<()>,
    release_first: mpsc::Sender<()>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl RecoveryServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("recovery listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking recovery listener");
        let endpoint = format!(
            "http://{}/v1/",
            listener.local_addr().expect("listener address")
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let (boundary_send, first_boundary) = mpsc::channel();
        let (release_first, release_receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            for index in 0..2 {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if stopping.load(Ordering::Acquire) || Instant::now() >= deadline {
                                return;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("recovery fixture accept: {error}"),
                    }
                };
                stream
                    .set_nonblocking(false)
                    .expect("blocking recovery stream");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("recovery read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("recovery write timeout");
                let request = read_request(&mut stream);
                recorded.lock().expect("recorded requests").push(request);
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                    )
                    .expect("recovery response headers");
                if index == 0 {
                    let frame = json!({
                        "choices":[{
                            "delta":{"content":"partial response before process loss"},
                            "finish_reason":null
                        }]
                    });
                    write!(stream, "data: {frame}\n\n").expect("partial recovery frame");
                    stream.flush().expect("flush partial recovery frame");
                    boundary_send.send(()).expect("publish HTTP boundary");
                    loop {
                        if release_receive.try_recv().is_ok()
                            || stopping.load(Ordering::Acquire)
                            || Instant::now() >= deadline
                        {
                            break;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    if stopping.load(Ordering::Acquire) {
                        return;
                    }
                } else {
                    let frame = json!({
                        "choices":[{
                            "delta":{"content":"fresh explicit task completed"},
                            "finish_reason":"stop"
                        }],
                        "usage":{"total_tokens":7}
                    });
                    write!(stream, "data: {frame}\n\ndata: [DONE]\n\n")
                        .expect("fresh recovery frame");
                    stream.flush().expect("flush fresh recovery frame");
                }
            }
        });
        Self {
            endpoint,
            requests,
            first_boundary,
            release_first,
            stop,
            worker: Some(worker),
        }
    }

    fn wait_for_first_boundary(&self) {
        self.first_boundary
            .recv_timeout(Duration::from_secs(15))
            .expect("first HTTP boundary");
    }

    fn release_first(&self) {
        let _ = self.release_first.send(());
    }

    fn request_count(&self) -> usize {
        self.requests.lock().expect("request count").len()
    }

    fn finish(mut self) -> Vec<Value> {
        self.release_first();
        self.worker
            .take()
            .expect("recovery worker")
            .join()
            .expect("recovery worker join");
        self.requests.lock().expect("requests").clone()
    }

    fn stop_after_first(mut self) -> Vec<Value> {
        self.stop.store(true, Ordering::Release);
        self.release_first();
        self.worker
            .take()
            .expect("recovery worker")
            .join()
            .expect("recovery worker join");
        self.requests.lock().expect("requests").clone()
    }
}

impl Drop for RecoveryServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.release_first.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn executable() -> PathBuf {
    std::env::var_os("ARGENTUM_F01_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_argentum")))
}

fn isolated_command(
    executable: &Path,
    root: &Path,
    workspace: &Path,
    database: &Path,
    args: &[&str],
) -> Command {
    let mut command = Command::new(executable);
    command
        .args(args)
        .args(["--isolated", "--no-color", "--workspace"])
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .current_dir(root)
        .env_clear();
    for variable in ["SystemRoot", "WINDIR", "SystemDrive", "PATH", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    command
}

fn run_isolated_executable(
    executable: &Path,
    root: &Path,
    workspace: &Path,
    database: &Path,
    args: &[&str],
) -> Output {
    isolated_command(executable, root, workspace, database, args)
        .stdin(Stdio::null())
        .output()
        .expect("isolated executable")
}

fn recovery_command(database: &Path, args: &[&str]) -> Output {
    let executable = executable();
    let mut command = Command::new(executable);
    command
        .arg("recovery")
        .args(args)
        .arg("--database")
        .arg(database)
        .arg("--json")
        .env_clear();
    for variable in ["SystemRoot", "WINDIR", "SystemDrive", "PATH", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    command.output().expect("recovery command")
}

fn json_output(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("single JSON output")
}

fn set_goal(fixture: &CliFixture) -> String {
    let value = json_output(&fixture.run(&[
        "goal",
        "set",
        "--objective",
        "F08 preserve unknown provider usage",
        "--token-budget",
        "500000",
        "--json",
    ]));
    value["goal"]["id"]
        .as_str()
        .expect("goal identity")
        .to_owned()
}

fn select_owner_full_access(fixture: &CliFixture) {
    tokio::runtime::Runtime::new()
        .expect("owner runtime")
        .block_on(async {
            let mut config = argentum_cli::HostConfig::in_memory(&fixture.workspace);
            config.database = Some(fixture.database.clone());
            let mut host = argentum_cli::CommandHost::start(config).expect("owner command host");
            host.local_handle()
                .dispatch(AppCommand::SelectExecutionProfile {
                    profile_id: "full-access".into(),
                })
                .await
                .expect("owner full access selection");
            host.shutdown().await.expect("owner host shutdown");
        });
}

fn wait_for_partial_boundary(database: &Path) -> RunId {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(inspection) = Store::inspect_recovery(database) {
            if let Some(run) = inspection.runs.iter().find(|run| {
                run.execution_state == RecoveryExecutionState::Active
                    && run.request_state == RecoveryRequestState::PartialResponse
            }) {
                assert_eq!(inspection.ownership.outcome, OwnershipOutcome::Live);
                return run.run_id;
            }
        }
        assert!(
            Instant::now() < deadline,
            "CLI did not persist the partial-response recovery boundary"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_compaction_dispatch(database: &Path) -> RunId {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(inspection) = Store::inspect_recovery(database) {
            if let Some(run) = inspection.runs.iter().find(|run| {
                run.execution_state == RecoveryExecutionState::Active
                    && run.kind == argentum_domain::recovery::RecoveryRunKind::Compaction
                    && run.request_state == RecoveryRequestState::Dispatched
            }) {
                assert_eq!(inspection.ownership.outcome, OwnershipOutcome::Live);
                return run.run_id;
            }
        }
        assert!(
            Instant::now() < deadline,
            "CLI did not persist the compaction dispatch boundary"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn complete_run_id(output: &Output) -> RunId {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<ResponseEnvelope>(line).ok())
        .find_map(|response| match response.payload {
            ServerPayload::Event { event } => match *event {
                AppEvent::RunStatusChanged {
                    run_id,
                    lifecycle: TaskLifecycle::Complete,
                } => Some(run_id),
                _ => None,
            },
            _ => None,
        })
        .expect("fresh complete run identity")
}

#[test]
fn killed_packaged_cli_reconciles_without_replay_and_allows_new_explicit_work() {
    let server = RecoveryServer::start();
    let fixture = CliFixture::new(&server.endpoint);
    let goal_id = set_goal(&fixture);
    let mut child = ChildGuard(
        fixture
            .command(&[
                "run",
                "--goal",
                &goal_id,
                "--prompt",
                "Original task that must never be replayed",
                "--json",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn packaged CLI"),
    );
    server.wait_for_first_boundary();
    let interrupted_run_id = wait_for_partial_boundary(&fixture.database);
    child.kill().expect("terminate owned CLI process");
    child.wait().expect("wait for owned CLI process");
    server.release_first();

    let before = json_output(&recovery_command(&fixture.database, &["inspect"]));
    assert_eq!(before["ownership"]["outcome"], "proven_abandoned");
    assert!(before["runs"].as_array().expect("runs").iter().any(|run| {
        run["run_id"] == interrupted_run_id.to_string()
            && run["execution_state"] == "active"
            && run["request_state"] == "partial_response"
    }));

    let settlement = json_output(&recovery_command(&fixture.database, &["reconcile"]));
    assert_eq!(settlement["mutation_performed"], true);
    assert_eq!(settlement["recovered_run_ids"], json!([interrupted_run_id]));
    let revision = settlement["recovery_revision"]
        .as_str()
        .expect("recovery revision")
        .to_owned();
    let recovered = json_output(&recovery_command(&fixture.database, &["inspect"]));
    let recovered_run = recovered["runs"]
        .as_array()
        .expect("recovered runs")
        .iter()
        .find(|run| run["run_id"] == interrupted_run_id.to_string())
        .expect("recovered run");
    assert_eq!(recovered_run["execution_state"], "interrupted");
    assert_eq!(recovered_run["accounting_state"], "unresolved");
    assert_eq!(recovered_run["authority_invalidated"], true);
    assert_eq!(recovered_run["admission_blocked"], true);
    assert!(recovered_run["unresolved_effects"]
        .as_array()
        .expect("uncertainties")
        .iter()
        .any(|reason| reason == "provider_request_outcome_unknown"));

    let review = json_output(&recovery_command(
        &fixture.database,
        &[
            "review",
            "--run-id",
            &interrupted_run_id.to_string(),
            "--revision",
            &revision,
        ],
    ));
    assert_eq!(review["admission_blocked"], false);
    assert_eq!(review["accounting_state"], "unresolved");

    let blocked_goal = fixture.run(&[
        "run",
        "--goal",
        &goal_id,
        "--prompt",
        "Do not refund or redispatch unknown usage",
        "--json",
    ]);
    assert!(!blocked_goal.status.success());
    assert_eq!(server.request_count(), 1);
    let goal = json_output(&fixture.run(&["goal", "status", "--json"]));
    assert!(
        goal["goal"]["accounting"]["counters"]["unresolved_tokens"]
            .as_u64()
            .expect("unresolved tokens")
            > 0
    );

    let fresh = fixture.run(&[
        "run",
        "--prompt",
        "Fresh explicit ordinary task after recovery",
        "--json",
    ]);
    assert!(
        fresh.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&fresh.stdout),
        String::from_utf8_lossy(&fresh.stderr)
    );
    let fresh_run_id = complete_run_id(&fresh);
    assert_ne!(fresh_run_id, interrupted_run_id);
    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    assert!(requests[0]
        .to_string()
        .contains("Original task that must never be replayed"));
    assert!(!requests[1]
        .to_string()
        .contains("partial response before process loss"));
    assert_eq!(
        requests[1]["messages"]
            .as_array()
            .expect("fresh messages")
            .last()
            .expect("fresh user message")["content"],
        "Fresh explicit ordinary task after recovery"
    );

    let final_state = Store::inspect_recovery(&fixture.database).expect("final recovery state");
    assert_eq!(
        final_state.ownership.outcome,
        OwnershipOutcome::NoActiveOwner
    );
    assert!(final_state.runs.iter().any(|run| {
        run.run_id == interrupted_run_id
            && run.execution_state == RecoveryExecutionState::Interrupted
            && run.accounting_state == RecoveryAccountingState::Unresolved
    }));
    assert!(final_state.runs.iter().any(|run| {
        run.run_id == fresh_run_id && run.execution_state == RecoveryExecutionState::Complete
    }));
}

#[test]
fn killed_supervised_attempt_retires_invocation_and_does_not_restart_it() {
    let server = RecoveryServer::start();
    let fixture = CliFixture::new(&server.endpoint);
    std::fs::create_dir(fixture.workspace.join("src")).expect("source directory");
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname = \"f08-supervisor-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    std::fs::write(
        fixture.workspace.join("src/lib.rs"),
        "pub fn answer()->u32{42}\n",
    )
    .expect("unformatted source");
    select_owner_full_access(&fixture);
    let set = json_output(&fixture.run(&[
        "goal",
        "set",
        "--objective",
        "Format the declared Rust source",
        "--token-budget",
        "500000",
        "--tool-budget",
        "8",
        "--time-budget",
        "300",
        "--json",
    ]));
    let goal_id = set["goal"]["id"]
        .as_str()
        .expect("goal identity")
        .to_owned();
    let criteria = fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &goal_id,
        "--runners",
        "cargo-fmt-check",
        "--json",
    ]);
    json_output(&criteria);

    let mut child = ChildGuard(
        fixture
            .command(&[
                "goal",
                "supervise",
                "--goal-id",
                &goal_id,
                "--iterations",
                "2",
                "--elapsed-seconds",
                "120",
                "--json",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn supervised CLI"),
    );
    server.wait_for_first_boundary();
    let run_id = wait_for_partial_boundary(&fixture.database);
    let live = Store::inspect_recovery(&fixture.database).expect("live supervisor inspection");
    let supervised = live
        .runs
        .iter()
        .find(|run| run.run_id == run_id)
        .expect("supervised run");
    assert_eq!(
        supervised.kind,
        argentum_domain::recovery::RecoveryRunKind::SupervisedAttempt
    );
    let connection = rusqlite::Connection::open(&fixture.database).expect("active invocation DB");
    let invocation_id: String = connection
        .query_row(
            "SELECT id FROM goal_invocations WHERE state = 'active'",
            [],
            |row| row.get(0),
        )
        .expect("active invocation");
    drop(connection);

    child.kill().expect("terminate supervised CLI");
    child.wait().expect("wait supervised CLI");
    server.release_first();
    let settlement = json_output(&recovery_command(&fixture.database, &["reconcile"]));
    assert_eq!(settlement["recovered_run_ids"], json!([run_id]));
    assert_eq!(
        settlement["recovered_invocation_ids"],
        json!([invocation_id.clone()])
    );
    let revision = settlement["recovery_revision"]
        .as_str()
        .expect("supervisor recovery revision")
        .to_owned();
    json_output(&recovery_command(
        &fixture.database,
        &[
            "review",
            "--run-id",
            &run_id.to_string(),
            "--revision",
            &revision,
        ],
    ));

    let connection = rusqlite::Connection::open(&fixture.database).expect("settled invocation DB");
    let (state, payload): (String, String) = connection
        .query_row(
            "SELECT state, payload FROM goal_invocations WHERE id = ?1",
            [invocation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("settled invocation");
    drop(connection);
    assert_eq!(state, "unresolved");
    let invocation: Value = serde_json::from_str(&payload).expect("invocation payload");
    assert_eq!(invocation["state"], "interrupted");
    assert_eq!(invocation["owner_claim_retired"], true);
    assert_eq!(invocation["stop_reason"], "owner_process_interrupted");

    let status = json_output(&fixture.run(&["goal", "status", "--json"]));
    assert_eq!(status["goal"]["lifecycle"], "paused");
    assert_eq!(server.request_count(), 1);
    assert_eq!(server.stop_after_first().len(), 1);
}

#[test]
fn killed_compaction_preserves_original_context_and_does_not_restart_summarization() {
    let seed_server = super::instructions_http::Server::start();
    let fixture = CliFixture::new(&seed_server.endpoint);
    super::working_context_http::compaction_http::seed(&seed_server, &fixture, 1_800);
    assert!(seed_server.requests.try_recv().is_err());
    drop(seed_server);
    let original_groups: usize = rusqlite::Connection::open(&fixture.database)
        .expect("context database")
        .query_row("SELECT COUNT(*) FROM context_groups", [], |row| row.get(0))
        .expect("original context groups");
    assert!(original_groups >= 4);

    let server = RecoveryServer::start();
    let saved = fixture.run(&[
        "provider",
        "save",
        "f01-fixture",
        "--label",
        "F08 compaction fixture",
        "--kind",
        "openai-compatible",
        "--endpoint",
        &server.endpoint,
        "--model",
        "fixture-model",
        "--select",
        "--json",
    ]);
    json_output(&saved);
    json_output(&fixture.run(&[
        "provider",
        "budget",
        "f01-fixture",
        "--limits",
        "2000000,1900000,8192,4096,1024",
        "--json",
    ]));
    let mut child = ChildGuard(
        fixture
            .command(&["compact", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn compaction CLI"),
    );
    server.wait_for_first_boundary();
    let run_id = wait_for_compaction_dispatch(&fixture.database);
    let live = Store::inspect_recovery(&fixture.database).expect("live compaction inspection");
    let compaction = live
        .runs
        .iter()
        .find(|run| run.run_id == run_id)
        .expect("compaction run");
    assert_eq!(
        compaction.kind,
        argentum_domain::recovery::RecoveryRunKind::Compaction
    );
    assert_eq!(compaction.request_purpose, "compaction_tools_disabled");

    child.kill().expect("terminate compaction CLI");
    child.wait().expect("wait compaction CLI");
    server.release_first();
    let settlement = json_output(&recovery_command(&fixture.database, &["reconcile"]));
    let revision = settlement["recovery_revision"]
        .as_str()
        .expect("compaction recovery revision")
        .to_owned();
    assert_eq!(settlement["recovered_run_ids"], json!([run_id]));
    let recovered = json_output(&recovery_command(&fixture.database, &["inspect"]));
    let compaction = recovered["runs"]
        .as_array()
        .expect("recovered runs")
        .iter()
        .find(|run| run["run_id"] == run_id.to_string())
        .expect("recovered compaction");
    assert_eq!(compaction["execution_state"], "interrupted");
    assert_eq!(compaction["request_purpose"], "compaction_tools_disabled");
    assert!(compaction["unresolved_effects"]
        .as_array()
        .expect("compaction uncertainties")
        .iter()
        .any(|reason| reason == "provider_request_outcome_unknown"));
    json_output(&recovery_command(
        &fixture.database,
        &[
            "review",
            "--run-id",
            &run_id.to_string(),
            "--revision",
            &revision,
        ],
    ));

    let context = json_output(&fixture.run(&["context", "--json"]));
    assert!(context["summary_checkpoint_id"].is_null());
    assert_eq!(server.request_count(), 1);
    let remaining_groups: usize = rusqlite::Connection::open(&fixture.database)
        .expect("reopened context database")
        .query_row("SELECT COUNT(*) FROM context_groups", [], |row| row.get(0))
        .expect("remaining context groups");
    assert_eq!(remaining_groups, original_groups);
    assert_eq!(server.stop_after_first().len(), 1);
}

#[test]
fn previous_exact_cli_leaves_stale_running_as_legacy_ownership_unknown() {
    let Some(previous) = std::env::var_os("ARGENTUM_F08_PREVIOUS_CLI_EXE").map(PathBuf::from)
    else {
        return;
    };
    assert!(previous.is_absolute() && previous.is_file());
    let server = RecoveryServer::start();
    let root = tempfile::tempdir().expect("legacy fixture");
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).expect("legacy workspace");
    let database = root.path().join("state.db");
    for args in [
        vec![
            "provider",
            "save",
            "f08-legacy",
            "--label",
            "F08 legacy fixture",
            "--kind",
            "openai-compatible",
            "--endpoint",
            &server.endpoint,
            "--model",
            "fixture-model",
            "--select",
            "--json",
        ],
        vec![
            "provider",
            "budget",
            "f08-legacy",
            "--limits",
            "2000000,1900000,8192,4096,1024",
            "--json",
        ],
    ] {
        let output = run_isolated_executable(&previous, root.path(), &workspace, &database, &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut child = ChildGuard(
        isolated_command(
            &previous,
            root.path(),
            &workspace,
            &database,
            &[
                "run",
                "--prompt",
                "Previous artifact stale running reproduction",
                "--json",
            ],
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn previous exact CLI"),
    );
    server.wait_for_first_boundary();
    let deadline = Instant::now() + Duration::from_secs(15);
    let stale_run_id = loop {
        if let Ok(connection) = rusqlite::Connection::open(&database) {
            let latest = connection.query_row(
                "SELECT payload FROM event_log WHERE event_type = 'run_status_changed' ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            );
            if let Ok(payload) = latest {
                if let Ok(AppEvent::RunStatusChanged {
                    run_id,
                    lifecycle: TaskLifecycle::Running,
                }) = serde_json::from_str::<AppEvent>(&payload)
                {
                    break run_id;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "previous CLI did not persist a Running event"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let live_unknown = Store::inspect_recovery(&database).expect("legacy live inspection");
    assert_eq!(
        live_unknown.ownership.outcome,
        OwnershipOutcome::OwnershipUnknown
    );
    assert!(live_unknown
        .legacy_ambiguities
        .iter()
        .any(|run| run.run_id == stale_run_id));

    child.kill().expect("terminate previous CLI");
    child.wait().expect("wait previous CLI");
    server.release_first();
    let killed_unknown = Store::inspect_recovery(&database).expect("legacy killed inspection");
    assert_eq!(
        killed_unknown.ownership.outcome,
        OwnershipOutcome::OwnershipUnknown
    );
    assert!(matches!(
        Store::reconcile_recovery(&database),
        Err(StoreError::HostOwnershipUnknown(
            "legacy_schema_without_durable_owner_generation"
        ))
    ));
    assert_eq!(server.stop_after_first().len(), 1);
}
