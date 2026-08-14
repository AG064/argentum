use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

use argentum_cli::protocol::{ResponseEnvelope, ServerPayload};
use argentum_domain::AppEvent;
use argentum_domain::ProviderProfile;

struct MockServer {
    endpoint: String,
    handle: thread::JoinHandle<()>,
}

impl MockServer {
    fn finish(self) {
        self.handle.join().expect("mock server");
    }
}

fn spawn_mock_server(status: &str, body: &str) -> MockServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let address = listener.local_addr().expect("mock address");
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept probe");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 2_048];
        loop {
            let count = stream.read(&mut buffer).expect("read probe");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        assert!(String::from_utf8_lossy(&request).starts_with("GET /v1/models HTTP/1.1\r\n"));
        stream.write_all(response.as_bytes()).expect("write probe");
    });
    MockServer {
        endpoint: format!("http://{address}/v1"),
        handle,
    }
}

fn run_probe(endpoint: &str, json: bool) -> Output {
    let workspace = tempfile::tempdir().expect("workspace");
    let database = workspace.path().join("state.sqlite3");
    let mut command = Command::new(env!("CARGO_BIN_EXE_argentum-cli"));
    command
        .arg("provider")
        .arg("probe")
        .arg("--workspace")
        .arg(workspace.path())
        .arg("--database")
        .arg(database)
        .arg("--endpoint")
        .arg(endpoint)
        .arg("--model")
        .arg("local/model")
        .env("RUST_LOG", "off");
    if json {
        command.arg("--json");
    }
    command.output().expect("run provider probe")
}

fn responses(output: &Output) -> Vec<ResponseEnvelope> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL response"))
        .collect()
}

#[test]
fn json_probe_success_exposes_connected_status_and_completed_terminal() {
    let server = spawn_mock_server(
        "200 OK",
        r#"{"object":"list","data":[{"id":"local/model"}]}"#,
    );

    let output = run_probe(&server.endpoint, true);
    server.finish();
    let responses = responses(&output);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(responses.iter().any(|response| matches!(
        &response.payload,
        ServerPayload::Event {
            event: AppEvent::ProviderStatus(status),
        } if status.connected && status.detail.contains("configured model: local/model")
    )));
    assert!(matches!(
        responses.last().map(|response| &response.payload),
        Some(ServerPayload::CommandCompleted)
    ));
}

#[test]
fn json_probe_failure_is_safe_and_terminates_command_failed() {
    let secret = "secret response body";
    let server = spawn_mock_server("401 Unauthorized", &format!(r#"{{"error":"{secret}"}}"#));

    let output = run_probe(&server.endpoint, true);
    server.finish();
    let responses = responses(&output);
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!output.status.success());
    assert!(responses.iter().any(|response| matches!(
        &response.payload,
        ServerPayload::Event {
            event: AppEvent::ProviderStatus(status),
        } if !status.connected && status.detail.contains("rejected the probe")
    )));
    assert!(responses.iter().any(|response| matches!(
        &response.payload,
        ServerPayload::Event {
            event: AppEvent::Error {
                recoverable: true,
                ..
            },
        }
    )));
    assert!(matches!(
        responses.last().map(|response| &response.payload),
        Some(ServerPayload::CommandFailed { .. })
    ));
    assert!(!rendered.contains(secret));
}

#[test]
fn human_probe_failure_prints_disconnected_state_and_exits_nonzero() {
    let server = spawn_mock_server("503 Service Unavailable", r#"{"error":"offline"}"#);

    let output = run_probe(&server.endpoint, false);
    server.finish();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stdout.contains("LM Studio: disconnected."));
    assert!(stdout.contains("returned HTTP 503"));
    assert!(stderr.contains("argentum-cli:"));
}

fn run_profile_command(
    workspace: &std::path::Path,
    database: &std::path::Path,
    arguments: &[&str],
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_argentum-cli"))
        .arg("provider")
        .args(arguments)
        .arg("--workspace")
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .arg("--json")
        .env("RUST_LOG", "off")
        .output()
        .expect("run provider profile command")
}

fn profile_output(output: &Output) -> Vec<ProviderProfile> {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "profile JSON: {error}; stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn profile_save_select_and_list_persist_across_cli_restarts() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("state.sqlite3");

    let saved = run_profile_command(
        &workspace,
        &database,
        &[
            "save",
            "local-secondary",
            "--label",
            "Secondary local",
            "--kind",
            "lm-studio",
            "--endpoint",
            "http://127.0.0.1:5678/v1",
            "--model",
            "secondary-model",
        ],
    );
    assert!(
        saved.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&saved.stderr)
    );
    let saved_profiles = profile_output(&saved);
    assert_eq!(saved_profiles.len(), 2);
    assert!(saved_profiles
        .iter()
        .any(|profile| profile.id == "lm-studio" && profile.selected));

    let selected = run_profile_command(&workspace, &database, &["select", "local-secondary"]);
    assert!(selected.status.success());
    assert!(profile_output(&selected)
        .iter()
        .any(|profile| profile.id == "local-secondary" && profile.selected));

    let model_selected = run_profile_command(
        &workspace,
        &database,
        &["model", "local-secondary", "--model", "secondary-model-v2"],
    );
    assert!(model_selected.status.success());
    assert!(profile_output(&model_selected).iter().any(|profile| {
        profile.id == "local-secondary" && profile.model == "secondary-model-v2" && profile.selected
    }));

    let listed = run_profile_command(&workspace, &database, &["list"]);
    assert!(listed.status.success());
    let listed_profiles = profile_output(&listed);
    assert_eq!(
        listed_profiles
            .iter()
            .filter(|profile| profile.selected)
            .count(),
        1
    );
    assert!(listed_profiles.iter().any(|profile| {
        profile.id == "local-secondary" && profile.model == "secondary-model-v2" && profile.selected
    }));
}

#[test]
fn invalid_profile_endpoint_is_rejected_without_disclosing_or_persisting_secret() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("state.sqlite3");
    let secret = "secret-query-value";

    let rejected = run_profile_command(
        &workspace,
        &database,
        &[
            "save",
            "unsafe-profile",
            "--label",
            "Unsafe profile",
            "--kind",
            "openai-compatible",
            "--endpoint",
            &format!("https://example.test/v1?api_key={secret}"),
            "--model",
            "test-model",
        ],
    );
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&rejected.stdout),
        String::from_utf8_lossy(&rejected.stderr)
    );

    assert!(!rejected.status.success());
    assert!(!rendered.contains(secret));
    let listed = run_profile_command(&workspace, &database, &["list"]);
    assert!(listed.status.success());
    assert!(!profile_output(&listed)
        .iter()
        .any(|profile| profile.id == "unsafe-profile"));
}
