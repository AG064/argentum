// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Agent modules through the whole runtime: off by default, switched on only
//! by the owner, and each host approved on its own. Every request goes to a
//! fake server on 127.0.0.1.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const WATCHDOG: Duration = Duration::from_secs(10);

struct PageServer {
    port: u16,
    hits: Arc<AtomicUsize>,
}

async fn page_server() -> PageServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buffer = vec![0_u8; 4096];
            let read = stream.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let body =
                format!("<html><title>Fixture</title><body><p>Page at {path}</p></body></html>");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    PageServer { port, hits }
}

struct CallOnce {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    call: Arc<Mutex<Option<ModelToolCall>>>,
}

#[async_trait]
impl ModelProvider for CallOnce {
    fn id(&self) -> &'static str {
        "module-script"
    }
    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let first = request.tool_exchanges.is_empty();
        self.requests.lock().unwrap().push(request);
        let call = if first {
            self.call.lock().unwrap().clone()
        } else {
            None
        };
        if let Some(call) = call {
            sender
                .send(ProviderEvent::ToolCall(call))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        } else {
            sender
                .send(ProviderEvent::Delta("done".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    owner: TestRuntime,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    call: Arc<Mutex<Option<ModelToolCall>>>,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let call = Arc::new(Mutex::new(None));
        let mut providers = ProviderRegistry::default();
        providers.register(CallOnce {
            requests: requests.clone(),
            call: call.clone(),
        });
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let owner = test_runtime(
            Store::open_in_memory().unwrap(),
            providers,
            tools,
            workspace,
        );
        configure_selected_provider(&owner, "module-script").await;
        Self {
            _directory: directory,
            owner,
            requests,
            call,
        }
    }

    fn fetch(&self, url: &str) {
        *self.call.lock().unwrap() = Some(ModelToolCall {
            id: format!("fetch-{}", Uuid::new_v4()),
            name: "web_fetch".into(),
            arguments: serde_json::json!({ "url": url }).to_string(),
        });
        self.requests.lock().unwrap().clear();
    }

    fn start(&self) -> tokio::task::JoinHandle<Result<(), RuntimeError>> {
        let runtime = self.owner.runtime.clone();
        tokio::spawn(async move {
            runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: "read the page".into(),
                })
                .await
        })
    }

    fn offered(&self) -> bool {
        self.requests.lock().unwrap()[0]
            .tools
            .iter()
            .any(|tool| tool.name == "web_fetch")
    }

    fn tool_result(&self) -> String {
        let requests = self.requests.lock().unwrap();
        requests
            .iter()
            .flat_map(|request| request.tool_exchanges.iter())
            .flat_map(|exchange| exchange.results.iter())
            .map(|result| result.content.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn turn_on(&self) {
        self.owner
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: "module.web-fetch".into(),
                enabled: true,
            })
            .await
            .expect("owner turns the module on");
        self.owner
            .dispatch(AppCommand::SetModuleSetting {
                module_id: "web-fetch".into(),
                key: "allow-local-addresses".into(),
                value: "true".into(),
            })
            .await
            .expect("owner allows local addresses");
    }
}

async fn next_approval(events: &mut broadcast::Receiver<AppEvent>) -> ApprovalRequest {
    tokio::time::timeout(WATCHDOG, async {
        loop {
            if let AppEvent::ApprovalRequested(approval) = events.recv().await.unwrap() {
                break approval;
            }
        }
    })
    .await
    .expect("approval watchdog")
}

async fn settle(
    run: tokio::task::JoinHandle<Result<(), RuntimeError>>,
) -> Result<(), RuntimeError> {
    tokio::time::timeout(WATCHDOG, run)
        .await
        .expect("run watchdog")
        .unwrap()
}

#[tokio::test]
async fn web_fetch_is_off_by_default_and_never_reaches_the_network() {
    let server = page_server().await;
    let fixture = Fixture::new().await;
    let mut events = fixture.owner.subscribe();
    fixture.fetch(&format!("http://127.0.0.1:{}/off", server.port));
    let _ = settle(fixture.start()).await;
    assert!(!fixture.offered(), "a module that is off offers no tools");
    assert_eq!(server.hits.load(Ordering::SeqCst), 0);
    assert!(!drain_events(&mut events)
        .iter()
        .any(|event| matches!(event, AppEvent::ApprovalRequested(_))));
}

#[tokio::test]
async fn module_switch_and_settings_need_local_owner_authority() {
    let fixture = Fixture::new().await;
    assert!(matches!(
        fixture
            .owner
            .runtime
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: "module.web-fetch".into(),
                enabled: true,
            })
            .await,
        Err(RuntimeError::LocalAuthorityRequired)
    ));
    assert!(matches!(
        fixture
            .owner
            .runtime
            .dispatch(AppCommand::SetModuleSetting {
                module_id: "web-search".into(),
                key: "search-address".into(),
                value: "https://search.example/".into(),
            })
            .await,
        Err(RuntimeError::LocalAuthorityRequired)
    ));
    assert!(fixture
        .owner
        .store
        .load_module_settings(fixture.owner.project_id)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn web_fetch_asks_once_per_host_and_returns_untrusted_text() {
    let server = page_server().await;
    let fixture = Fixture::new().await;
    fixture.turn_on().await;
    let mut events = fixture.owner.subscribe();

    fixture.fetch(&format!("http://127.0.0.1:{}/first", server.port));
    let run = fixture.start();
    let approval = next_approval(&mut events).await;
    assert_eq!(approval.tool_id, "web_fetch");
    assert_eq!(approval.target, "web host 127.0.0.1");
    assert!(approval.reason.contains("/first"), "{}", approval.reason);
    assert_eq!(approval.capabilities, vec![Capability::Network]);
    assert_eq!(
        server.hits.load(Ordering::SeqCst),
        0,
        "nothing before approval"
    );
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Action,
        })
        .await
        .unwrap();
    settle(run).await.unwrap();
    assert!(fixture.offered());
    let result = fixture.tool_result();
    assert!(result.contains("untrusted data"), "{result}");
    assert!(result.contains("Page at /first"), "{result}");
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);

    // The same host on another path needs no new approval.
    drain_events(&mut events);
    fixture.fetch(&format!("http://127.0.0.1:{}/second?x=1", server.port));
    settle(fixture.start()).await.unwrap();
    assert!(!drain_events(&mut events)
        .iter()
        .any(|event| matches!(event, AppEvent::ApprovalRequested(_))));
    assert!(fixture.tool_result().contains("Page at /second?x=1"));
    assert_eq!(server.hits.load(Ordering::SeqCst), 2);

    // Another host asks again.
    fixture.fetch(&format!("http://localhost:{}/third", server.port));
    let run = fixture.start();
    let approval = next_approval(&mut events).await;
    assert_eq!(approval.target, "web host localhost");
    assert_eq!(server.hits.load(Ordering::SeqCst), 2);
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    settle(run).await.unwrap();
    assert!(fixture.tool_result().contains("Page at /third"));
}

#[tokio::test]
async fn a_session_approval_for_the_network_covers_one_host() {
    let server = page_server().await;
    let fixture = Fixture::new().await;
    fixture.turn_on().await;
    let mut events = fixture.owner.subscribe();
    fixture.fetch(&format!("http://127.0.0.1:{}/a", server.port));
    let run = fixture.start();
    let approval = next_approval(&mut events).await;
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Session,
        })
        .await
        .unwrap();
    settle(run).await.unwrap();
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    let (grants, _) = fixture
        .owner
        .store
        .approval_grants_history_page(fixture.owner.project_id, 100)
        .unwrap();
    let network = grants
        .iter()
        .filter(|grant| grant.capability == Capability::Network)
        .collect::<Vec<_>>();
    assert_eq!(network.len(), 1);
    assert_eq!(network[0].scope, GrantScope::Action);

    // Another host in the same session asks again.
    drain_events(&mut events);
    fixture.fetch(&format!("http://localhost:{}/b", server.port));
    let run = fixture.start();
    let approval = next_approval(&mut events).await;
    assert_eq!(approval.target, "web host localhost");
    fixture
        .owner
        .dispatch(AppCommand::RejectTool {
            approval_id: approval.id,
        })
        .await
        .unwrap();
    let _ = settle(run).await;
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);
}

fn picture(name: &str) -> argentum_domain::ImageAttachment {
    use base64::Engine;
    let mut png = vec![
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d,
    ];
    png.extend(b"IHDR");
    png.extend(2_u32.to_be_bytes());
    png.extend(2_u32.to_be_bytes());
    png.extend([8, 6, 0, 0, 0]);
    argentum_domain::ImageAttachment {
        name: name.into(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(png),
    }
}

impl Fixture {
    async fn submit_with_pictures(&self, prompt: &str) -> Result<(), RuntimeError> {
        *self.call.lock().unwrap() = None;
        self.requests.lock().unwrap().clear();
        let runtime = self.owner.runtime.clone();
        let prompt = prompt.to_owned();
        settle(tokio::spawn(async move {
            runtime
                .dispatch(AppCommand::SubmitTaskWithImages {
                    prompt,
                    images: vec![picture("screen.png")],
                })
                .await
        }))
        .await
    }

    async fn turn_on_images(&self, list_model: bool) {
        self.owner
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: "module.images".into(),
                enabled: true,
            })
            .await
            .expect("owner turns images on");
        if list_model {
            self.owner
                .dispatch(AppCommand::SetModuleSetting {
                    module_id: "images".into(),
                    key: "image-models".into(),
                    value: "other-model, test-model".into(),
                })
                .await
                .expect("owner lists the model");
        }
    }
}

#[tokio::test]
async fn pictures_need_the_images_module_and_a_model_that_reads_them() {
    let fixture = Fixture::new().await;
    assert!(matches!(
        fixture.submit_with_pictures("what is this?").await,
        Err(RuntimeError::ImagesRefused(_))
    ));
    assert!(fixture.requests.lock().unwrap().is_empty());

    fixture.turn_on_images(false).await;
    assert!(matches!(
        fixture.submit_with_pictures("what is this?").await,
        Err(RuntimeError::ImagesRefused(_))
    ));
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn pictures_go_with_their_run_only_and_are_not_saved() {
    let fixture = Fixture::new().await;
    fixture.turn_on_images(true).await;
    fixture
        .submit_with_pictures("what is this?")
        .await
        .expect("run with a picture");
    {
        let requests = fixture.requests.lock().unwrap();
        assert!(!requests.is_empty());
        for request in requests.iter() {
            assert_eq!(request.prompt_images.len(), 1);
            assert_eq!(request.prompt_images[0].media_type(), "image/png");
            assert!(request.prompt.contains("[1 picture attached: screen.png]"));
        }
    }
    let encoded = picture("screen.png").data_base64;
    let events = fixture
        .owner
        .store
        .events_for_workspace(fixture._directory.path())
        .unwrap();
    assert!(!serde_json::to_string(&events).unwrap().contains(&encoded));

    // The next run without pictures carries none.
    let runtime = fixture.owner.runtime.clone();
    fixture.requests.lock().unwrap().clear();
    settle(tokio::spawn(async move {
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "and now?".into(),
            })
            .await
    }))
    .await
    .expect("run without pictures");
    assert!(fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request.prompt_images.is_empty()));
    assert!(fixture.owner.prompt_images.lock().unwrap().is_empty());
}
