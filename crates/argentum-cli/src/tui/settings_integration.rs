use super::*;
use std::io::{BufRead, BufReader};
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

struct CatalogServer {
    endpoint: String,
    calls: Arc<AtomicUsize>,
    accepted: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl CatalogServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let accepted = Arc::new(AtomicUsize::new(0));
        let accepted_connections = accepted.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(WATCHDOG)).unwrap();
                stream.set_write_timeout(Some(WATCHDOG)).unwrap();
                accepted_connections.fetch_add(1, Ordering::Release);
                let mut reader = BufReader::new(&mut stream);
                let mut first = String::new();
                assert_ne!(reader.read_line(&mut first).unwrap(), 0, "empty request");
                assert_eq!(
                    first, "GET /v1/models HTTP/1.1\r\n",
                    "settings must not start inference"
                );
                loop {
                    let mut line = String::new();
                    assert_ne!(
                        reader.read_line(&mut line).unwrap(),
                        0,
                        "incomplete headers"
                    );
                    if line == "\r\n" {
                        break;
                    }
                    assert!(!line.to_ascii_lowercase().starts_with("authorization:"));
                }
                count.fetch_add(1, Ordering::SeqCst);
                let body =
                    json!({"object":"list","data":[{"id":"fixture"},{"id":"second"}]}).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        Self {
            endpoint,
            calls,
            accepted,
            stop,
            worker: Some(worker),
        }
    }
}

#[test]
fn catalog_fixture_reads_a_fragmented_request_after_accept() {
    let server = CatalogServer::start();
    let address = server
        .endpoint
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/v1/")
        .unwrap()
        .parse()
        .unwrap();
    let mut stream = std::net::TcpStream::connect_timeout(&address, WATCHDOG).unwrap();
    stream.set_read_timeout(Some(WATCHDOG)).unwrap();
    stream.set_write_timeout(Some(WATCHDOG)).unwrap();
    let deadline = Instant::now() + WATCHDOG;
    while server.accepted.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(server.accepted.load(Ordering::Acquire), 1);
    // The accepted worker has entered read_line with no complete line yet.
    stream.write_all(b"GET /v1/models HTTP/1.1").unwrap();
    thread::sleep(Duration::from_millis(50));
    stream
        .write_all(b"\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    std::io::Read::read_to_string(&mut stream, &mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
}
impl Drop for CatalogServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}
async fn open(h: &mut Harness, command: &str, marker: &str) {
    h.input.send(InputEvent::Help).unwrap();
    h.input.send(InputEvent::Text(command.into())).unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains(marker)).await;
}

#[tokio::test]
async fn reliability_session_picker_preserves_each_draft_without_model_requests() {
    let server = CatalogServer::start();
    let mut h = Harness::start(&server.endpoint).await;
    h.input.send(InputEvent::Text("draft A".into())).unwrap();
    for (id, expected) in [(h.b, ""), (h.a, "draft A")] {
        let index = h
            .control
            .workspace_snapshot()
            .unwrap()
            .sessions
            .iter()
            .position(|s| s.id == id)
            .unwrap();
        open(&mut h, "sessions", "Sessions | settings").await;
        for _ in 0..index {
            h.input.send(InputEvent::HistoryNext).unwrap();
        }
        h.input.send(InputEvent::Enter).unwrap();
        let v = h.view(|v| v.selected == Some(id) && !v.switching).await;
        assert_eq!(v.draft, expected);
    }
    let (_, tasks, terminals) = h.finish().await;
    assert!(tasks.is_empty() && terminals.is_empty());
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reliability_verifier_picker_runs_fixed_command_after_explicit_confirmation() {
    let server = CatalogServer::start();
    let mut h = Harness::start(&server.endpoint).await;
    let root = h._directory.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"menu-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn answer() -> u32 {\n    42\n}\n",
    )
    .unwrap();
    h.control
        .apply_access_review(h.control.access_review("verification").unwrap())
        .await
        .unwrap();
    h.input
        .send(InputEvent::Text("keep verifier draft".into()))
        .unwrap();
    open(&mut h, "verification", "Verification | settings").await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Run cargo-fmt-check in this workspace"))
        .await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Hold F8")).await;
    h.input.send(InputEvent::ConfirmAccess(true)).unwrap();
    h.view(|v| v.status == "Settings updated. Draft preserved.")
        .await;
    let mut passed = false;
    while let Ok(event) = h.events.try_recv() {
        if let AppEvent::VerificationEvidenceRecorded(e) = event {
            passed |= e.runner == argentum_domain::VerificationRunnerId::CargoFmtCheck
                && e.state == argentum_domain::VerificationState::Passed;
        }
    }
    assert!(passed);
    let (t, tasks, _) = h.finish().await;
    assert_eq!(t.composer, "keep verifier draft");
    assert!(tasks.is_empty());
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reliability_provider_model_and_context_selection_use_production_input_without_submitting_draft(
) {
    let server = CatalogServer::start();
    let mut h = Harness::start(&server.endpoint).await;
    h.control
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "other".into(),
                label: "Other".into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                endpoint: server.endpoint.clone(),
                model: "fixture".into(),
                selected: false,
            },
        })
        .await
        .unwrap();
    h.control
        .declare_model_budget("other", [2_000_000, 1_900_000, 8192, 4096, 1024])
        .unwrap();
    let index = h
        .control
        .provider_profiles()
        .unwrap()
        .iter()
        .position(|p| p.id == "other")
        .unwrap();
    h.input
        .send(InputEvent::Text("keep this unsent draft".into()))
        .unwrap();
    open(&mut h, "providers", "Providers | settings").await;
    for _ in 0..index {
        h.input.send(InputEvent::HistoryNext).unwrap();
    }
    h.input.send(InputEvent::Enter).unwrap();
    let provider = h
        .view(|v| v.provider == "Other" && v.review.contains("Model | settings"))
        .await;
    assert_eq!(provider.draft, "keep this unsent draft");
    h.input.send(InputEvent::Escape).unwrap();
    h.view(|v| v.review.trim().is_empty()).await;
    open(&mut h, "models", "second").await;
    h.input.send(InputEvent::HistoryNext).unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    let model = h
        .view(|v| v.model == "second" && v.review.contains("Context | settings"))
        .await;
    assert_eq!(model.draft, "keep this unsent draft");
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Enter saves")).await;
    h.input
        .send(InputEvent::Text("32768,30000,4096,2048,1024".into()))
        .unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Limits: 32768,30000,4096,2048,1024"))
        .await;
    assert_eq!(
        h.control
            .selected_model_budget()
            .unwrap()
            .unwrap()
            .usable_context_tokens,
        32768
    );
    h.input.send(InputEvent::Escape).unwrap();
    h.view(|v| v.review.trim().is_empty()).await;
    open(&mut h, "setup", "Setup | settings").await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Providers | settings")).await;
    h.input.send(InputEvent::Escape).unwrap();
    h.view(|v| v.review.contains("Setup | settings")).await;
    h.input.send(InputEvent::Escape).unwrap();
    let final_view = h.view(|v| v.review.trim().is_empty()).await;
    assert_eq!(final_view.draft, "keep this unsent draft");
    assert!(server.calls.load(Ordering::SeqCst) > 0);
    let (_, tasks, terminals) = h.finish().await;
    assert!(tasks.is_empty() && terminals.is_empty());
}
