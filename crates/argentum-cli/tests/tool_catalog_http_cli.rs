use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use argentum_cli::protocol::{RequestEnvelope, ResponseEnvelope, ServerPayload};
use argentum_domain::{AppCommand, AppEvent, TaskLifecycle};
use serde_json::{json, Value};
#[path = "support/http.rs"]
mod http_support;
use http_support::read_request;
#[path = "support/accounting_recovery_http.rs"]
mod accounting_recovery_http;
#[path = "support/goal_accounting_http.rs"]
mod goal_accounting_http;
#[path = "support/goal_supervision_http.rs"]
mod goal_supervision_http;
#[path = "support/instructions_http.rs"]
mod instructions_http;
#[path = "support/native_delivery_http.rs"]
mod native_delivery_http;
#[path = "support/recovery_http.rs"]
mod recovery_http;
#[path = "support/verification_diagnostics_http.rs"]
mod verification_diagnostics_http;
#[path = "support/working_context_http.rs"]
mod working_context_http;

const FILE_CONTENT: &str = "F01 real disposable file content 7b920a\n";
const EXPECTED_TOOLS: &[&str] = &[
    "apply_patch",
    "cancel_process_job",
    "copy_file",
    "create_directory",
    "delete_file",
    "edit_text",
    "execute_argv",
    "execute_shell",
    "file_metadata",
    "git_read",
    "git_write",
    "list_directory",
    "move_file",
    "poll_process_job",
    "read_binary_metadata",
    "read_text",
    "search_filenames",
    "search_text",
    "start_verification",
    "verify",
    "write_text",
];

struct ChildGuard(Child);

impl std::ops::Deref for ChildGuard {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct HttpFixture {
    endpoint: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    request_notifications: mpsc::Receiver<()>,
}

#[derive(Clone, Copy)]
enum ApprovalAction {
    Edit,
    Process,
    Git,
}

impl HttpFixture {
    fn start_public_checks(case: PublicCheckCase) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (request_notice, request_notifications) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(240);
            while !stopping.load(Ordering::Acquire) && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("synthetic boundary server: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let request = read_request(&mut stream);
                let index = recorded.lock().unwrap().len();
                recorded.lock().unwrap().push(request.clone());
                let _ = request_notice.send(());
                let frames = public_check_frames(index, &request, case);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
                for frame in frames {
                    stream
                        .write_all(format!("data: {frame}\n\n").as_bytes())
                        .unwrap();
                }
                stream.write_all(b"data: [DONE]\n\n").unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            worker: Some(worker),
            request_notifications,
        }
    }
    fn start_usability() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (request_notice, request_notifications) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !stopping.load(Ordering::Acquire) && Instant::now() < deadline {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("synthetic fixture accept: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let request = read_request(&mut stream);
                let index = recorded.lock().unwrap().len();
                recorded.lock().unwrap().push(request.clone());
                let _ = request_notice.send(());
                let frames = usability_frames(index, &request);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
                for frame in frames {
                    stream
                        .write_all(format!("data: {}\n\n", frame).as_bytes())
                        .unwrap();
                }
                stream.write_all(b"data: [DONE]\n\n").unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            worker: Some(worker),
            request_notifications,
        }
    }

    fn start(stall: bool) -> Self {
        Self::start_scenario(stall, false)
    }

    fn start_scenario(stall: bool, large_file: bool) -> Self {
        Self::start_mode(stall, large_file, false)
    }

    fn start_mode(stall: bool, large_file: bool, approval_required: bool) -> Self {
        Self::start_config(
            stall,
            large_file,
            approval_required.then_some(ApprovalAction::Edit),
            false,
        )
    }

    fn start_config(
        stall: bool,
        large_file: bool,
        approval_action: Option<ApprovalAction>,
        reuse: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (request_notice, request_notifications) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !stopping.load(Ordering::Acquire) && Instant::now() < deadline {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("fixture accept: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let request = read_request(&mut stream);
                let index = recorded.lock().unwrap().len();
                recorded.lock().unwrap().push(request.clone());
                let _ = request_notice.send(());
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
                stream.flush().unwrap();
                if stall {
                    while !stopping.load(Ordering::Acquire) && Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(5));
                    }
                    break;
                }
                let frames = if large_file {
                    large_file_frames(index, &request)
                } else if index == 0 {
                    vec![
                        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"f01-read","type":"function","function":{"name":"read_","arguments":"{\"pa"}}]},"finish_reason":null}]}),
                        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"text","arguments":"th\":\"fixture."}}]},"finish_reason":null}]}),
                        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"txt\"}"}}]},"finish_reason":null}]}),
                        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
                    ]
                } else if let Some(action) = approval_action.filter(|_| index == 1) {
                    let (name, args) = match action {
                        ApprovalAction::Edit => (
                            "edit_text",
                            json!({"path":"fixture.txt", "expected_sha256":tool_reply(&request, "f01-read")["sha256"],
                            "old_text":FILE_CONTENT, "new_text":"F03 unauthorized replacement\r\n"}),
                        ),
                        ApprovalAction::Process => (
                            "execute_shell",
                            json!({"command":"echo F03 > child-marker.txt", "timeout_ms":1000}),
                        ),
                        ApprovalAction::Git => (
                            "git_write",
                            json!({"operation":{"kind":"add","paths":["fixture.txt"]}}),
                        ),
                    };
                    vec![
                        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"f03-edit","type":"function",
                        "function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]}),
                    ]
                } else {
                    vec![
                        json!({"choices":[{"delta":{"content":"F01 round trip complete."},"finish_reason":"stop"}]}),
                    ]
                };
                for frame in frames {
                    let frame = format!("data: {frame}\n\n");
                    // Exercise both SSE framing and fragmented tool-name/argument deltas.
                    for fragment in frame.as_bytes().chunks(11) {
                        stream.write_all(fragment).unwrap();
                    }
                    stream.flush().unwrap();
                }
                stream.write_all(b"data: [DONE]\n\n").unwrap();
                if index
                    == if large_file {
                        4
                    } else if reuse {
                        2
                    } else {
                        1
                    }
                {
                    break;
                }
            }
        });
        Self {
            endpoint,
            requests,
            stop,
            worker: Some(worker),
            request_notifications,
        }
    }

    fn finish(mut self) -> Vec<Value> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("HTTP fixture worker");
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct CliFixture {
    root: tempfile::TempDir,
    workspace: PathBuf,
    database: PathBuf,
}

impl CliFixture {
    fn new(endpoint: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("fixture.txt"), FILE_CONTENT).unwrap();
        let fixture = Self {
            workspace,
            database: root.path().join("state.db"),
            root,
        };
        let saved = fixture.run(&[
            "provider",
            "save",
            "f01-fixture",
            "--label",
            "F01 fixture",
            "--kind",
            "openai-compatible",
            "--endpoint",
            endpoint,
            "--model",
            "fixture-model",
            "--select",
            "--json",
        ]);
        assert!(
            saved.status.success(),
            "{}",
            String::from_utf8_lossy(&saved.stderr)
        );
        let declared = fixture.run(&[
            "provider",
            "budget",
            "f01-fixture",
            "--limits",
            "2000000,1900000,8192,4096,1024",
            "--json",
        ]);
        assert!(
            declared.status.success(),
            "{}",
            String::from_utf8_lossy(&declared.stderr)
        );
        let harness = fixture.run(&["harness", "status", "--json"]);
        assert!(harness.status.success());
        let state: Value = serde_json::from_slice(&harness.stdout).unwrap();
        assert_eq!(state["harness"]["selected_execution_profile_id"], "guided");
        fixture
    }

    fn command(&self, args: &[&str]) -> Command {
        let executable = std::env::var_os("ARGENTUM_F01_CLI_EXE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_argentum")));
        assert!(executable.is_absolute() && executable.is_file());
        let mut command = Command::new(executable);
        command
            .args(args)
            .args(["--isolated", "--no-color", "--workspace"])
            .arg(&self.workspace)
            .arg("--database")
            .arg(&self.database)
            .current_dir(self.root.path())
            .env_clear();
        for variable in ["SystemRoot", "WINDIR", "SystemDrive", "PATH", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(variable) {
                command.env(variable, value);
            }
        }
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_stdin(args, false)
    }

    fn run_with_stdin(&self, args: &[&str], keep_open: bool) -> Output {
        let mut child = ChildGuard(
            self.command(args)
                .stdin(if keep_open {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let _open_stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout = thread::spawn(move || read_output(stdout));
        let stderr = thread::spawn(move || read_output(stderr));
        let status = wait_child(&mut child);
        Output {
            status,
            stdout: stdout.join().unwrap(),
            stderr: stderr.join().unwrap(),
        }
    }
}

fn read_output(input: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    input.take(4 * 1024 * 1024).read_to_end(&mut bytes).unwrap();
    bytes
}

fn wait_child(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("CLI fixture timed out");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn assert_catalog(request: &Value) {
    let mut names: Vec<_> = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["function"]["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, EXPECTED_TOOLS);
}

#[test]
fn headless_guided_edit_reports_approval_required_and_settles() {
    let server = HttpFixture::start_mode(false, false, true);
    let fixture = CliFixture::new(&server.endpoint);
    let preserved = fixture.prepare_approval_repository();
    let started = Instant::now();
    let output = fixture.run(&[
        "run",
        "--prompt",
        "Read fixture.txt and edit its exact contents.",
        "--json",
    ]);
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(15));
    assert_eq!(output.status.code(), Some(1));
    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_catalog(request);
    }
    assert_eq!(
        tool_reply(&requests[1], "f01-read")["content"],
        FILE_CONTENT
    );
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.last().unwrap()["type"], "command_failed");
    assert_eq!(replies.last().unwrap()["code"], "approval_required");
    assert_eq!(
        std::fs::read(fixture.workspace.join("fixture.txt")).unwrap(),
        FILE_CONTENT.as_bytes()
    );
    assert!(!replies
        .iter()
        .any(|reply| reply["type"] == "command_completed"));
    assert_failed_run_and_correlation(&replies, "run-1");
    fixture.assert_approval_repository_unchanged(&preserved);
    fixture.assert_failed_trajectory();
    println!("F03: default Guided read/edit; approval_required; process exited in {} ms without approval or watchdog termination", elapsed.as_millis());
}

fn assert_failed_run_and_correlation(replies: &[Value], request_id: &str) {
    let failures = replies
        .iter()
        .filter(|reply| reply["type"] == "command_failed" && reply["request_id"] == request_id)
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    let failure = failures[0];
    assert_eq!(failure["code"], "approval_required");
    assert_eq!(failure["recoverable"], false);
    assert!(failure["run_id"].is_string());
    assert!(failure["tool_call_id"].is_string());
    assert!(failure["approval_id"].is_string());
    let run_id = failure["run_id"].clone();
    let events = replies
        .iter()
        .filter_map(|reply| serde_json::from_value::<ResponseEnvelope>(reply.clone()).ok())
        .collect::<Vec<_>>();
    let terminal = events
        .iter()
        .filter_map(|reply| match &reply.payload {
            ServerPayload::Event { event } => match event.as_ref() {
                AppEvent::RunStatusChanged {
                    run_id: id,
                    lifecycle,
                } if serde_json::json!(id) == run_id && lifecycle.is_terminal() => Some(*lifecycle),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(terminal, [TaskLifecycle::Failed]);
    assert!(!events.iter().any(|reply| matches!(&reply.payload, ServerPayload::Event { event } if matches!(event.as_ref(), AppEvent::ApprovalRequested(_)))));
}

impl CliFixture {
    fn prepare_approval_repository(&self) -> (Vec<u8>, Vec<u8>) {
        std::fs::write(self.workspace.join("fixture.txt"), "staged baseline\r\n").unwrap();
        std::fs::write(self.workspace.join("unrelated.txt"), "staged unrelated\r\n").unwrap();
        for args in [
            &["init", "--quiet"][..],
            &["add", "--", "fixture.txt", "unrelated.txt"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&self.workspace)
                .output()
                .unwrap()
                .status
                .success());
        }
        std::fs::write(self.workspace.join("fixture.txt"), FILE_CONTENT).unwrap();
        std::fs::write(
            self.workspace.join("unrelated.txt"),
            "unstaged user edit\r\n",
        )
        .unwrap();
        let enable = self.run(&[
            "harness",
            "module",
            "first-party.trajectory",
            "enable",
            "--json",
        ]);
        assert!(
            enable.status.success(),
            "{}",
            String::from_utf8_lossy(&enable.stderr)
        );
        (
            std::fs::read(self.workspace.join(".git/index")).unwrap(),
            std::fs::read(self.workspace.join(".git/HEAD")).unwrap(),
        )
    }

    fn assert_approval_repository_unchanged(&self, preserved: &(Vec<u8>, Vec<u8>)) {
        assert_eq!(
            std::fs::read(self.workspace.join("fixture.txt")).unwrap(),
            FILE_CONTENT.as_bytes()
        );
        assert_eq!(
            std::fs::read(self.workspace.join("unrelated.txt")).unwrap(),
            b"unstaged user edit\r\n"
        );
        assert_eq!(
            std::fs::read(self.workspace.join(".git/index")).unwrap(),
            preserved.0
        );
        assert_eq!(
            std::fs::read(self.workspace.join(".git/HEAD")).unwrap(),
            preserved.1
        );
        assert!(!self.workspace.join("child-marker.txt").exists());
    }

    fn assert_failed_trajectory(&self) {
        let output = self.run(&["harness", "trajectory", "--json"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let wrapped: Value = serde_json::from_slice(&output.stdout).unwrap();
        let snapshot: argentum_domain::TrajectorySnapshot =
            serde_json::from_value(wrapped["trajectory"].clone()).unwrap();
        assert!(snapshot.entries.iter().any(|entry| entry.kind
            == argentum_domain::TrajectoryKind::Lifecycle
            && entry.state == argentum_domain::TrajectoryState::Error));
    }
}

#[test]
fn headless_open_pipe_plain_and_structured_process_git_and_edit_settle() {
    for action in [
        ApprovalAction::Edit,
        ApprovalAction::Process,
        ApprovalAction::Git,
    ] {
        for json_output in [false, true] {
            let server = HttpFixture::start_config(false, false, Some(action), false);
            let fixture = CliFixture::new(&server.endpoint);
            let preserved = fixture.prepare_approval_repository();
            let args = if json_output {
                vec![
                    "run",
                    "--prompt",
                    "Read the fixture and perform the requested action.",
                    "--json",
                ]
            } else {
                vec![
                    "run",
                    "--prompt",
                    "Read the fixture and perform the requested action.",
                ]
            };
            let output = fixture.run_with_stdin(&args, true);
            assert_eq!(output.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&output.stderr).contains("approval_required"));
            if json_output {
                let replies: Vec<Value> = String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                assert_failed_run_and_correlation(&replies, "run-1");
            } else {
                assert!(String::from_utf8_lossy(&output.stderr).contains("argentum chat"));
                assert!(!String::from_utf8_lossy(&output.stdout).contains("Approve?"));
            }
            let requests = server.finish();
            assert_eq!(requests.len(), 2);
            for request in &requests {
                assert_catalog(request);
            }
            fixture.assert_approval_repository_unchanged(&preserved);
            fixture.assert_failed_trajectory();
        }
    }
    println!("F03: open stdin pipe; plain/JSON edit, process and Git; all failed approval_required; persisted Failed; no effects");
}

struct ServeProcess {
    child: ChildGuard,
    input: Option<std::process::ChildStdin>,
    replies: mpsc::Receiver<Value>,
    reader: thread::JoinHandle<()>,
    errors: thread::JoinHandle<Vec<u8>>,
}

impl ServeProcess {
    fn start(fixture: &CliFixture) -> Self {
        let mut child = ChildGuard(
            fixture
                .command(&["serve"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let errors = thread::spawn(move || read_output(stderr));
        let (sender, replies) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let value: Value =
                    serde_json::from_str(&line.unwrap()).expect("parseable JSONL stdout");
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            replies,
            reader,
            errors,
        }
    }

    fn send(&mut self, id: &str, command: AppCommand) {
        let input = self.input.as_mut().unwrap();
        writeln!(
            input,
            "{}",
            serde_json::to_string(&RequestEnvelope::command(id, command)).unwrap()
        )
        .unwrap();
        input.flush().unwrap();
    }

    fn collect_until(&self, replies: &mut Vec<Value>, done: impl Fn(&[Value]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !done(replies) {
            assert!(replies.len() < 1000);
            replies.push(
                self.replies
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("JSONL settlement watchdog"),
            );
        }
    }

    fn finish(mut self, replies: &mut Vec<Value>) {
        drop(self.input.take());
        let status = wait_child(&mut self.child);
        self.reader.join().unwrap();
        replies.extend(self.replies.try_iter());
        let errors = self.errors.join().unwrap();
        assert!(status.success(), "{}", String::from_utf8_lossy(&errors));
    }
}

fn terminal_count(replies: &[Value], expected: TaskLifecycle) -> usize {
    replies.iter().filter(|reply| matches!(serde_json::from_value::<ResponseEnvelope>((*reply).clone()).unwrap().payload,
        ServerPayload::Event { event } if matches!(event.as_ref(), AppEvent::RunStatusChanged { lifecycle, .. } if *lifecycle == expected))).count()
}

#[test]
fn jsonl_approval_required_settles_then_fresh_request_succeeds_without_authority_expansion() {
    let server = HttpFixture::start_config(false, false, Some(ApprovalAction::Edit), true);
    let fixture = CliFixture::new(&server.endpoint);
    let preserved = fixture.prepare_approval_repository();
    let mut cli = ServeProcess::start(&fixture);
    let mut replies = Vec::new();
    cli.send(
        "f03-run",
        AppCommand::SubmitTask {
            prompt: "Read and edit the fixture".into(),
        },
    );
    cli.collect_until(&mut replies, |replies| {
        replies
            .iter()
            .any(|r| r["request_id"] == "f03-run" && r["type"] == "command_failed")
            && terminal_count(replies, TaskLifecycle::Failed) == 1
    });
    assert_failed_run_and_correlation(&replies, "f03-run");
    let failure = replies
        .iter()
        .find(|r| r["request_id"] == "f03-run" && r["type"] == "command_failed")
        .unwrap();
    let approval_id = serde_json::from_value(failure["approval_id"].clone()).unwrap();
    cli.send(
        "f03-untrusted-approval",
        AppCommand::ApproveTool {
            approval_id,
            scope: argentum_domain::ApprovalScope::Once,
        },
    );
    cli.collect_until(&mut replies, |replies| {
        replies.iter().any(|r| {
            r["request_id"] == "f03-untrusted-approval" && r["code"] == "command_not_permitted"
        })
    });
    cli.send(
        "f03-next",
        AppCommand::SubmitTask {
            prompt: "Return a short final answer without using tools.".into(),
        },
    );
    cli.collect_until(&mut replies, |replies| {
        replies
            .iter()
            .any(|r| r["request_id"] == "f03-next" && r["type"] == "command_completed")
            && terminal_count(replies, TaskLifecycle::Complete) == 1
    });
    cli.finish(&mut replies);
    assert_failed_run_and_correlation(&replies, "f03-run");
    assert_eq!(terminal_count(&replies, TaskLifecycle::Failed), 1);
    assert_eq!(terminal_count(&replies, TaskLifecycle::Complete), 1);
    assert_eq!(
        replies
            .iter()
            .filter(|r| r["request_id"] == "f03-next" && r["type"] == "command_completed")
            .count(),
        1
    );
    let requests = server.finish();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_catalog(request);
    }
    fixture.assert_approval_repository_unchanged(&preserved);
    fixture.assert_failed_trajectory();
    println!("F03 JSONL: Failed run and one approval_required reply; approval mutation denied; fresh task Complete; EOF clean; no effects");
}

#[test]
fn jsonl_eof_cancels_owned_http_work_without_claiming_approval_required() {
    let server = HttpFixture::start(true);
    let fixture = CliFixture::new(&server.endpoint);
    let mut cli = ServeProcess::start(&fixture);
    cli.send(
        "f03-eof",
        AppCommand::SubmitTask {
            prompt: "Wait for the provider.".into(),
        },
    );
    server
        .request_notifications
        .recv_timeout(Duration::from_secs(15))
        .expect("HTTP admission watchdog");
    drop(cli.input.take());
    let mut replies = Vec::new();
    cli.finish(&mut replies);
    assert_eq!(terminal_count(&replies, TaskLifecycle::Cancelled), 1);
    assert_eq!(terminal_count(&replies, TaskLifecycle::Failed), 0);
    assert!(!replies.iter().any(|r| r["code"] == "approval_required"));
    assert_eq!(
        replies
            .iter()
            .filter(|r| {
                r["request_id"] == "f03-eof"
                    && r["type"] == "command_failed"
                    && r["code"] == "command_cancelled"
            })
            .count(),
        1
    );
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert_catalog(&requests[0]);
    println!("F03 EOF: admitted HTTP work Cancelled, command settled as cancelled, server exited without external termination");
}

const TARGET_OFFSET: usize = 350_000;
const OLD_TARGET: &str = "let target = 41;";
const NEW_TARGET: &str = "let target = 42;";
const OMITTED_MARKER: &str = "F02_MIDDLE_MUST_NOT_REACH_PROVIDER";

fn tool_reply(request: &Value, call_id: &str) -> Value {
    let message = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "tool" && message["tool_call_id"] == call_id)
        .unwrap_or_else(|| panic!("missing result for {call_id}"));
    serde_json::from_str(message["content"].as_str().unwrap()).expect("structured tool result")
}

fn large_file_frames(index: usize, request: &Value) -> Vec<Value> {
    let (id, name, args) = match index {
        0 => ("f02-initial", "read_text", json!({"path":"large.rs"})),
        1 => (
            "f02-tail",
            "read_text",
            json!({"path":"large.rs", "start_byte":TARGET_OFFSET,
            "expected_sha256":tool_reply(request, "f02-initial")["sha256"]}),
        ),
        2 => (
            "f02-edit",
            "edit_text",
            json!({"path":"large.rs", "old_text":OLD_TARGET, "new_text":NEW_TARGET,
            "expected_sha256":tool_reply(request, "f02-tail")["sha256"]}),
        ),
        3 => (
            "f02-verify",
            "read_text",
            json!({"path":"large.rs", "start_byte":TARGET_OFFSET,
            "expected_sha256":tool_reply(request, "f02-edit")["sha256"]}),
        ),
        4 => {
            return vec![
                json!({"choices":[{"delta":{"content":"F02 targeted edit verified."},"finish_reason":"stop"}]}),
            ]
        }
        _ => panic!("unexpected F02 HTTP request"),
    };
    vec![
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":id,"type":"function",
        "function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]}),
    ]
}

fn usability_frames(index: usize, request: &Value) -> Vec<Value> {
    let (id, name, args) = match index {
        0 => (
            "invalid-search",
            "search_text",
            json!({"path":"src","query":"marker","max_results":20,"mode":"wildcard"}),
        ),
        1 => {
            let result = tool_reply(request, "invalid-search");
            assert_eq!(result["executed"], false);
            assert_eq!(result["issues"][0]["field"], "mode");
            (
                "valid-search",
                "search_text",
                json!({"path":"src","query":"marker","max_results":20}),
            )
        }
        2 => (
            "context",
            "read_text",
            json!({"path":"src/lib.rs","start_line":4096,"line_count":3}),
        ),
        3 => {
            let read = tool_reply(request, "context");
            assert_eq!(read["line_count"], Value::Null); // selectors are in edit_selection, not an inferred target.
            (
                "mixed-edit",
                "edit_text",
                json!({"path":"src/lib.rs","expected_sha256":read["sha256"],
                "old_text":"leaf","new_text":"branch","start_line":4097,"line_count":1,
                "replacement_lines":["pub fn marker() -> &'static str { r\"path\\branch\" }"]}),
            )
        }
        4 => {
            let result = tool_reply(request, "mixed-edit");
            assert_eq!(result["executed"], false);
            assert_eq!(result["issues"][0]["category"], "edit_form");
            (
                "narrow",
                "read_text",
                json!({"path":"src/lib.rs","start_line":4097,"line_count":1}),
            )
        }
        5 => {
            let read = tool_reply(request, "narrow");
            let selection = read["edit_selection"]
                .as_object()
                .expect("complete narrow selection");
            let mut args = selection.clone();
            args.insert("path".into(), "src/lib.rs".into());
            args.insert(
                "replacement_lines".into(),
                json!(["pub fn marker() -> &'static str { r\"path\\branch\" }"]),
            );
            ("valid-edit", "edit_text", Value::Object(args))
        }
        6 => (
            "fixed-verify",
            "verify",
            json!({"runners":["cargo-test-locked"]}),
        ),
        7 => {
            return vec![
                json!({"choices":[{"delta":{"content":"Synthetic contract flow settled."},"finish_reason":"stop"}]}),
            ]
        }
        8 => {
            let history = request["messages"].to_string();
            assert!(
                history.contains("mixed_edit_forms"),
                "specific retained category did not reach a fresh request"
            );
            return vec![
                json!({"choices":[{"delta":{"content":"Retained correction observed."},"finish_reason":"stop"}]}),
            ];
        }
        _ => panic!("unexpected synthetic contract request"),
    };
    vec![
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":id,"type":"function",
        "function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]}),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PublicCheckCase {
    GuardedRepair,
    CorrectExpectation,
    FalseSuccess,
    AdmissionFailure,
}
fn public_check_frames(index: usize, request: &Value, case: PublicCheckCase) -> Vec<Value> {
    const CODE: &str = "#[test] fn public_value(){assert_eq!(boundary_fixture::value(),8);}";
    let (id, name, args) = match index {
        0 => ("source", "read_text", json!({"path":"src/lib.rs"})),
        1 => (
            "ordinary-before",
            "verify",
            json!({"runners":["cargo-test-locked"]}),
        ),
        2 => {
            assert_eq!(tool_reply(request, "ordinary-before")[0]["state"], "passed");
            (
                "counterexample",
                "check_rust",
                json!({"package":if case==PublicCheckCase::AdmissionFailure{"wrong-package"}else{"boundary_fixture"},"profile":"debug","test_source":CODE}),
            )
        }
        3 => {
            if matches!(
                case,
                PublicCheckCase::FalseSuccess | PublicCheckCase::AdmissionFailure
            ) {
                return vec![
                    json!({"choices":[{"delta":{"content":"All requirements passed. Everything is complete."},"finish_reason":"stop"}]}),
                ];
            }
            let result = tool_reply(request, "counterexample");
            assert_eq!(result["supplemental_check"]["state"], "failed");
            assert_eq!(result["project_suite_ran"], false);
            assert!(result["feedback"]["stdout"]
                .as_str()
                .unwrap()
                .contains("public_value"));
            if case == PublicCheckCase::CorrectExpectation {
                return vec![
                    json!({"choices":[{"delta":{"content":"The prior expectation was wrong: the public contract retains value 7. Correcting that expectation, not changing the source."}}]}),
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"recheck","type":"function","function":{"name":"check_rust","arguments":json!({"package":"boundary_fixture","test_source":"#[test]fn public_value(){assert_eq!(boundary_fixture::value(),7);}","replaces_check_id":result["supplemental_check"]["id"]}).to_string()}}]},"finish_reason":"tool_calls"}]}),
                ];
            }
            (
                "repair",
                "edit_text",
                json!({"path":"src/lib.rs","expected_sha256":tool_reply(request,"source")["sha256"],"old_text":"pub fn value()->u32{7}","new_text":"pub fn value()->u32{8}"}),
            )
        }
        4 if case == PublicCheckCase::CorrectExpectation => {
            assert_eq!(
                tool_reply(request, "recheck")["supplemental_check"]["state"],
                "reported_pass"
            );
            (
                "ordinary-after",
                "verify",
                json!({"runners":["cargo-test-locked"]}),
            )
        }
        5 if case == PublicCheckCase::CorrectExpectation => {
            assert_eq!(tool_reply(request, "ordinary-after")[0]["state"], "passed");
            return vec![
                json!({"choices":[{"delta":{"content":"Corrected the wrong supplemental expectation and ran the requested suite. The initial failed check remains in the record; this is not independent proof of all requirements."},"finish_reason":"stop"}]}),
            ];
        }
        4 => (
            "recheck",
            "check_rust",
            json!({"package":"boundary_fixture","profile":"debug","test_source":CODE}),
        ),
        5 => {
            assert_eq!(
                tool_reply(request, "recheck")["supplemental_check"]["state"],
                "reported_pass"
            );
            (
                "ordinary-after",
                "verify",
                json!({"runners":["cargo-test-locked"]}),
            )
        }
        6 => {
            assert_eq!(tool_reply(request, "ordinary-after")[0]["state"], "passed");
            return vec![
                json!({"choices":[{"delta":{"content":"The scoped source repair, model-authored public assertion and requested project suite ran on the final source. This is not proof of every natural-language requirement."},"finish_reason":"stop"}]}),
            ];
        }
        _ => panic!("unexpected synthetic boundary continuation"),
    };
    vec![
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]}),
    ]
}

impl CliFixture {
    fn allow_public_checks_via_owner(&self) {
        self.allow_usability_tools_via_owner();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut config = argentum_cli::HostConfig::in_memory(&self.workspace);
            config.database = Some(self.database.clone());
            let mut host = argentum_cli::CommandHost::start(config).unwrap();
            host.local_handle()
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: "verification.supplemental-rust".into(),
                    enabled: true,
                })
                .await
                .unwrap();
            host.shutdown().await.unwrap();
        });
    }
    fn allow_usability_tools_via_owner(&self) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut config = argentum_cli::HostConfig::in_memory(&self.workspace);
            config.database = Some(self.database.clone());
            let mut host = argentum_cli::CommandHost::start(config).unwrap();
            let owner = host.local_handle();
            owner
                .dispatch(AppCommand::SelectExecutionProfile {
                    profile_id: "verification".into(),
                })
                .await
                .unwrap();
            for tool in host.tool_descriptors() {
                // Fixed verification must not depend on generic argv exposure.
                if !matches!(
                    tool.id.as_str(),
                    "read_text"
                        | "search_text"
                        | "search_filenames"
                        | "edit_text"
                        | "verify"
                        | "start_verification"
                        | "poll_process_job"
                        | "cancel_process_job"
                ) {
                    owner
                        .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                            capability_id: tool.capability_id,
                            enabled: false,
                        })
                        .await
                        .unwrap();
                }
            }
            let policy = host.execution_policy().unwrap();
            assert_eq!(
                policy.capability_enabled.get("execution.argv"),
                Some(&false)
            );
            assert_eq!(
                policy.capability_enabled.get("verification.runner"),
                Some(&true)
            );
            assert_eq!(policy.source_profile_id.as_deref(), Some("verification"));
            host.shutdown().await.unwrap();
        });
    }
    fn allow_only_text_tools_via_owner(&self) {
        // Trusted test owner, supported local controls, private disposable database.
        // No raw SQL, JSONL approval, binary test hook or default-policy change.
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut config = argentum_cli::HostConfig::in_memory(&self.workspace);
            config.database = Some(self.database.clone());
            let mut host = argentum_cli::CommandHost::start(config).unwrap();
            let owner = host.local_handle();
            owner
                .dispatch(AppCommand::SelectExecutionProfile {
                    profile_id: "automatic".into(),
                })
                .await
                .unwrap();
            let mut tool_caps = host
                .tool_descriptors()
                .into_iter()
                .map(|tool| tool.capability_id)
                .collect::<Vec<_>>();
            // Stable ordering, independent of generic-command enablement.
            tool_caps.sort_by_key(|id| usize::from(id != "verification.runner"));
            for capability_id in tool_caps {
                if !matches!(capability_id.as_str(), "tool.read-text" | "tool.edit-text") {
                    owner
                        .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                            capability_id,
                            enabled: false,
                        })
                        .await
                        .unwrap();
                }
            }
            let policy = host.execution_policy().unwrap();
            assert_eq!(
                policy.sandbox_scope,
                argentum_domain::SandboxScope::WorkspaceWrite
            );
            assert_eq!(policy.approval_mode, argentum_domain::ApprovalMode::Never);
            host.shutdown().await.unwrap();
        });
    }
}

#[test]
fn first_attempt_streamed_contract_correction_edit_and_fixed_verification_preserve_bytes() {
    let server = HttpFixture::start_usability();
    let fixture = CliFixture::new(&server.endpoint);
    fixture.allow_usability_tools_via_owner();
    std::fs::create_dir_all(fixture.workspace.join("src")).unwrap();
    let before = format!(
        "{}pub fn marker() -> &'static str {{ r\"path\\leaf\" }}\r\n// neighbor é中 unchanged\r\n",
        "// context untouched\r\n".repeat(4096)
    );
    let after = before.replacen("path\\leaf", "path\\branch", 1);
    std::fs::write(fixture.workspace.join("src/lib.rs"), &before).unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname=\"harbor-marker\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.lock"),
        "version=4\n\n[[package]]\nname=\"harbor-marker\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("protected.txt"),
        b"OWNER_UNCHANGED\r\n",
    )
    .unwrap();
    let output = fixture.run(&[
        "run",
        "--prompt",
        "Update only the observed marker and perform fixed verification.",
        "--json",
    ]);
    let fresh = fixture.run(&[
        "run",
        "--prompt",
        "Review the retained correction category without changing files.",
        "--json",
    ]);
    let requests = server.finish();
    assert!(
        output.status.success(),
        "child CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fresh.status.success(),
        "fresh child request failed: {}",
        String::from_utf8_lossy(&fresh.stderr)
    );
    assert_eq!(requests.len(), 9);
    assert_eq!(
        std::fs::read(fixture.workspace.join("src/lib.rs")).unwrap(),
        after.as_bytes()
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join("protected.txt")).unwrap(),
        b"OWNER_UNCHANGED\r\n"
    );
    for request in &requests {
        let definitions = request["tools"].as_array().unwrap();
        assert!(!definitions.iter().any(|t| matches!(
            t["function"]["name"].as_str(),
            Some("execute_argv" | "execute_shell")
        )));
        let edit = definitions
            .iter()
            .find(|t| t["function"]["name"] == "edit_text")
            .unwrap();
        assert_eq!(
            edit["function"]["parameters"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    let verified = tool_reply(&requests[7], "fixed-verify");
    let receipts = verified
        .as_array()
        .expect("real typed fixed-verifier receipts");
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts[0]["state"], "passed",
        "fixed verification did not pass: {verified}"
    );
    assert_eq!(receipts[0]["exit_code"], 0);
    // Independently grade a byte-copy, without repairing the agent postimage.
    let grade = fixture.root.path().join("independent-grade");
    std::fs::create_dir_all(grade.join("src")).unwrap();
    for name in ["Cargo.toml", "Cargo.lock", "src/lib.rs"] {
        std::fs::copy(fixture.workspace.join(name), grade.join(name)).unwrap();
    }
    std::fs::create_dir(grade.join("tests")).unwrap();
    std::fs::write(grade.join("tests/contract.rs"),"#[test] fn marker_contract() { assert_eq!(harbor_marker::marker(), r\"path\\branch\"); }\n").unwrap();
    let graded = Command::new(std::env::var_os("CARGO").expect("installed Cargo"))
        .args(["test", "--locked", "--offline", "-j", "1"])
        .current_dir(&grade)
        .output()
        .unwrap();
    assert!(
        graded.status.success(),
        "independent source-copy grade failed: {}",
        String::from_utf8_lossy(&graded.stderr)
    );
}

#[test]
fn model_authored_public_check_failure_guarded_repair_and_current_verification_cli() {
    public_check_cli_case(PublicCheckCase::GuardedRepair);
}
#[test]
fn public_check_incorrect_expectation_is_corrected_without_relabeling() {
    public_check_cli_case(PublicCheckCase::CorrectExpectation);
}
#[test]
fn public_check_known_failure_cannot_be_hidden_by_final_prose() {
    public_check_cli_case(PublicCheckCase::FalseSuccess);
}
#[test]
fn public_check_admission_failure_cannot_be_hidden_by_final_prose() {
    public_check_cli_case(PublicCheckCase::AdmissionFailure);
}
fn public_check_cli_case(case: PublicCheckCase) {
    let server = HttpFixture::start_public_checks(case);
    let fixture = CliFixture::new(&server.endpoint);
    fixture.allow_public_checks_via_owner();
    std::fs::create_dir(fixture.workspace.join("src")).unwrap();
    std::fs::create_dir(fixture.workspace.join("tests")).unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname='boundary_fixture'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"boundary_fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("src/lib.rs"),
        "pub fn value()->u32{7}\n",
    )
    .unwrap();
    let protected = b"#[test]fn visible(){assert!(boundary_fixture::value()>0);}\n";
    std::fs::write(fixture.workspace.join("tests/visible.rs"), protected).unwrap();
    let mut command=fixture.command(&["run","--prompt","Make value return 8 without changing project tests. Use genuine public checks and requested verification.","--final-result"]);
    for variable in ["RUSTUP_HOME", "RUSTUP_TOOLCHAIN", "USERPROFILE"] {
        if let Some(v) = std::env::var_os(variable) {
            command.env(variable, v);
        }
    }
    let mut child = ChildGuard(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || read_output(stdout));
    let err = thread::spawn(move || read_output(stderr));
    let deadline = Instant::now() + Duration::from_secs(240);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "bounded boundary CLI watchdog");
        thread::sleep(Duration::from_millis(10));
    };
    let output = out.join().unwrap();
    let errors = err.join().unwrap();
    let requests = server.finish();
    let blocked = matches!(
        case,
        PublicCheckCase::FalseSuccess | PublicCheckCase::AdmissionFailure
    );
    assert_eq!(
        status.success(),
        !blocked,
        "boundary CLI: {}",
        String::from_utf8_lossy(&errors)
    );
    assert_eq!(
        requests.len(),
        if blocked {
            4
        } else if case == PublicCheckCase::CorrectExpectation {
            6
        } else {
            7
        }
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join("src/lib.rs")).unwrap(),
        if case == PublicCheckCase::GuardedRepair {
            b"pub fn value()->u32{8}\n"
        } else {
            b"pub fn value()->u32{7}\n"
        }
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join("tests/visible.rs")).unwrap(),
        protected
    );
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        result["run_state"],
        if blocked { "failed" } else { "complete" }
    );
    if case == PublicCheckCase::AdmissionFailure {
        assert_eq!(
            result["supplemental_checks"].as_array().map_or(0, Vec::len),
            0
        );
    } else {
        assert_eq!(
            result["supplemental_checks"].as_array().unwrap().len(),
            if blocked { 1 } else { 2 }
        );
        assert_eq!(result["supplemental_checks"][0]["state"], "failed");
        if !blocked {
            assert_eq!(result["supplemental_checks"][1]["state"], "reported_pass");
        }
        if case == PublicCheckCase::GuardedRepair {
            assert_ne!(
                result["supplemental_checks"][0]["source_revision"],
                result["supplemental_checks"][1]["source_revision"]
            );
        }
        if case == PublicCheckCase::CorrectExpectation {
            assert_eq!(
                result["supplemental_checks"][0]["source_revision"],
                result["supplemental_checks"][1]["source_revision"]
            );
            assert_eq!(
                result["supplemental_checks"][1]["replacements"],
                result["supplemental_checks"][0]["id"]
            );
        }
    }
    for request in requests {
        let tools = request["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["function"]["name"] == "check_rust"));
        assert!(!tools.iter().any(|t| matches!(
            t["function"]["name"].as_str(),
            Some("execute_argv" | "execute_shell")
        )));
    }
}

#[test]
fn packaged_large_file_read_jump_edit_verify_preserves_unseen_and_index_bytes() {
    let server = HttpFixture::start_scenario(false, true);
    let fixture = CliFixture::new(&server.endpoint);
    fixture.allow_only_text_tools_via_owner();
    let prelude = "*/\r\nfn main() {\r\n";
    let mut original = format!(
        "/*{}{prelude}",
        "x".repeat(TARGET_OFFSET - 2 - prelude.len())
    );
    original.replace_range(150_000..150_000 + OMITTED_MARKER.len(), OMITTED_MARKER);
    original.push_str(OLD_TARGET);
    original.push_str("\r\n// UTF-8: é中\r\n}\r\n");
    let expected = original.replacen(OLD_TARGET, NEW_TARGET, 1);
    std::fs::write(fixture.workspace.join("large.rs"), &original).unwrap();
    std::fs::write(
        fixture.workspace.join("unrelated.txt"),
        "staged baseline\r\n",
    )
    .unwrap();
    for args in [
        &["init", "--quiet"][..],
        &["add", "--", "large.rs", "unrelated.txt"],
    ] {
        let result = Command::new("git")
            .args(args)
            .current_dir(&fixture.workspace)
            .output()
            .unwrap();
        assert!(result.status.success(), "fixture Git setup failed");
    }
    std::fs::write(
        fixture.workspace.join("unrelated.txt"),
        "unrelated unstaged user change\r\n",
    )
    .unwrap();
    let index_before = std::fs::read(fixture.workspace.join(".git/index")).unwrap();
    let output = fixture.run(&[
        "run",
        "--prompt",
        "Inspect large.rs, jump near its end, replace the target and verify.",
        "--json",
    ]);
    let requests = server.finish();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(requests.len(), 5);
    for request in &requests {
        let names = request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, ["edit_text", "read_text"]);
        let system = request["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("sandbox: WorkspaceWrite; approval mode: Never"));
        assert!(system.contains("responder available: false"));
        assert!(!system.contains("File writes always require"));
        let wire = request.to_string();
        assert!(!wire.contains(OMITTED_MARKER));
        assert!(!wire.contains(&original));
        // Even the final history with repeated earlier results is below file size.
        assert!(wire.len() < original.len());
        for message in request["messages"].as_array().unwrap() {
            if message["role"] == "tool" {
                assert!(message["content"].as_str().unwrap().len() <= 64 * 1024);
            }
            if let Some(calls) = message["tool_calls"].as_array() {
                for call in calls {
                    assert!(call["function"]["arguments"].as_str().unwrap().len() < 512);
                }
            }
        }
    }
    let initial = tool_reply(&requests[1], "f02-initial");
    assert_eq!(initial["schema_version"], 1);
    assert_eq!(initial["requested_start_byte"], 0);
    assert_eq!(initial["requested_end_byte"], original.len());
    assert_eq!(initial["returned_start_byte"], 0);
    assert_eq!(
        initial["returned_end_byte"],
        initial["content"].as_str().unwrap().len()
    );
    assert_eq!(initial["range_complete"], false);
    assert_eq!(initial["is_complete_file"], false);
    assert_eq!(initial["file_has_more"], true);
    assert_eq!(initial["next_start_byte"], initial["returned_end_byte"]);
    assert_eq!(initial["file_size_bytes"], original.len());
    let tail = tool_reply(&requests[2], "f02-tail");
    assert_eq!(tail["returned_start_byte"], TARGET_OFFSET);
    assert_eq!(tail["content"], &original[TARGET_OFFSET..]);
    assert_eq!(tail["sha256"], initial["sha256"]);
    assert_eq!(tail["range_complete"], true);
    assert_eq!(tail["is_complete_file"], false);
    assert_eq!(tail["file_has_more"], false);
    let edit = tool_reply(&requests[3], "f02-edit");
    assert_eq!(edit["replaced_matches"], 1);
    assert_ne!(edit["sha256"], tail["sha256"]);
    let verified = tool_reply(&requests[4], "f02-verify");
    assert_eq!(verified["sha256"], edit["sha256"]);
    assert_eq!(verified["content"], &expected[TARGET_OFFSET..]);
    assert_eq!(
        std::fs::read(fixture.workspace.join("large.rs")).unwrap(),
        expected.as_bytes()
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join("unrelated.txt")).unwrap(),
        b"unrelated unstaged user change\r\n"
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join(".git/index")).unwrap(),
        index_before
    );
    let responses: Vec<ResponseEnvelope> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(responses.iter().any(|response| matches!(&response.payload, ServerPayload::Event { event }
        if matches!(event.as_ref(), AppEvent::RunStatusChanged { lifecycle: TaskLifecycle::Complete, .. }))));
    assert!(matches!(
        responses.last().map(|r| &r.payload),
        Some(ServerPayload::CommandCompleted)
    ));
    println!("F02: {} byte source; 5 HTTP requests; bounded initial read, direct tail jump, unique authorized edit, readback; only intended bytes changed; index and unrelated edit preserved", original.len());
}

#[test]
fn default_guided_catalog_round_trips_through_production_http_adapter() {
    let server = HttpFixture::start(false);
    let fixture = CliFixture::new(&server.endpoint);
    let output = fixture.run(&[
        "run",
        "--prompt",
        "Read fixture.txt using read_text, then finish.",
        "--json",
    ]);
    let requests = server.finish();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        requests.len(),
        2,
        "initial request and tool-result continuation"
    );
    for request in &requests {
        assert_catalog(request);
    }
    assert!(requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["role"] == "tool"
            && message["tool_call_id"] == "f01-read"
            && serde_json::from_str::<Value>(message["content"].as_str().unwrap()).unwrap()
                ["content"]
                == FILE_CONTENT));
    let responses: Vec<ResponseEnvelope> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(responses.iter().any(|response| matches!(&response.payload, ServerPayload::Event { event }
        if matches!(event.as_ref(), AppEvent::RunStatusChanged { lifecycle: TaskLifecycle::Complete, .. }))));
    assert!(matches!(
        responses.last().map(|r| &r.payload),
        Some(ServerPayload::CommandCompleted)
    ));
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("fixture.txt")).unwrap(),
        FILE_CONTENT
    );
    println!("F01: Guided; all {} tools; 2 HTTP requests; real file result received; run Complete; command completed", EXPECTED_TOOLS.len());
}

#[test]
fn default_guided_http_run_cancels_without_a_late_completion() {
    let server = HttpFixture::start(true);
    let fixture = CliFixture::new(&server.endpoint);
    let mut child = ChildGuard(
        fixture
            .command(&["serve"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let errors = thread::spawn(move || read_output(stderr));
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let response: ResponseEnvelope = serde_json::from_str(&line.unwrap()).unwrap();
            if sender.send(response).is_err() {
                break;
            }
        }
    });
    let request = RequestEnvelope::command(
        "f01-run",
        AppCommand::SubmitTask {
            prompt: "Wait for cancellation.".into(),
        },
    );
    writeln!(input, "{}", serde_json::to_string(&request).unwrap()).unwrap();
    input.flush().unwrap();
    let run_id = loop {
        let response = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        if let ServerPayload::Event { event } = response.payload {
            if let AppEvent::RunStatusChanged {
                run_id,
                lifecycle: TaskLifecycle::Running,
            } = *event
            {
                break run_id;
            }
        }
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.requests.lock().unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "initial HTTP request did not arrive"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let cancel = RequestEnvelope::command("f01-cancel", AppCommand::CancelRun { run_id });
    writeln!(input, "{}", serde_json::to_string(&cancel).unwrap()).unwrap();
    input.flush().unwrap();
    let mut cancelled = false;
    let mut command_cancelled = false;
    loop {
        let response = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        if let ServerPayload::Event { event } = &response.payload {
            assert!(!matches!(
                event.as_ref(),
                AppEvent::RunStatusChanged {
                    lifecycle: TaskLifecycle::Complete,
                    ..
                }
            ));
            if matches!(
                event.as_ref(),
                AppEvent::RunStatusChanged {
                    lifecycle: TaskLifecycle::Cancelled,
                    ..
                }
            ) {
                cancelled = true;
            }
        }
        if response.request_id.as_deref() == Some("f01-run")
            && matches!(
                response.payload,
                ServerPayload::CommandFailed { ref code, .. } if code == "command_cancelled"
            )
        {
            command_cancelled = true;
        }
        if cancelled && command_cancelled {
            break;
        }
    }
    assert!(cancelled);
    drop(input);
    assert!(wait_child(&mut child).success());
    reader.join().unwrap();
    for response in receiver.try_iter() {
        if let ServerPayload::Event { event } = response.payload {
            assert!(!matches!(
                *event,
                AppEvent::RunStatusChanged {
                    lifecycle: TaskLifecycle::Complete,
                    ..
                } | AppEvent::AssistantDelta { .. }
            ));
        }
    }
    assert!(errors.join().unwrap().is_empty());
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert_catalog(&requests[0]);
    println!(
        "F01 cancellation: complete Guided catalog reached HTTP; run Cancelled; process exited"
    );
}
